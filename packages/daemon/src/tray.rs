//! System tray for `nodespaced` (ADR-031).
//!
//! Owns the menu-bar / notification-area icon and acts as the platform-wide
//! UI launcher. The tray is the only path that fully shuts down NodeSpace —
//! closing the Tauri window terminates the UI process only; the daemon keeps
//! running with the tray visible.
//!
//! Threading: the `tao` event loop must run on the main thread (macOS
//! `NSApplication` is main-thread-only), so the tonic gRPC server runs on a
//! worker tokio runtime and signals back via [`TrayController::shutdown`].

use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

/// PNG used for the menu-bar icon. 32×32 is large enough that macOS, Windows
/// and Linux all downscale gracefully; we keep one asset rather than shipping
/// a per-platform set since the daemon's footprint should stay small.
const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/tray-icon.png");

/// Events the tonic side of the daemon can push into the tray event loop.
///
/// `MenuEvent` is forwarded verbatim from `tray-icon`'s global channel so the
/// `tao` loop can process menu clicks. `RpcStateChanged` is how the gRPC
/// layer reports activity for the live Status label.
enum TrayEvent {
    Menu(MenuEvent),
    RpcStateChanged,
}

/// Handle the gRPC side of the daemon uses to talk to the tray.
///
/// `shutdown` resolves once when the user picks "Quit" so the tonic server
/// can drain and exit. The RPC counters drive the live Status label.
#[derive(Clone)]
pub struct TrayController {
    proxy: EventLoopProxy<TrayEvent>,
    quit_notify: Arc<tokio::sync::Notify>,
    active_rpcs: Arc<AtomicUsize>,
}

impl TrayController {
    /// Future that resolves when the user selects "Quit". Pass this to
    /// `tonic::transport::Server::serve_with_shutdown` so the gRPC server
    /// exits cleanly before the tray closes.
    pub async fn shutdown(&self) {
        self.quit_notify.notified().await;
    }

    /// Record that an RPC just started. Pair with [`Self::rpc_completed`] —
    /// the difference is what the Status menu shows.
    pub fn rpc_started(&self) {
        self.active_rpcs.fetch_add(1, Ordering::Relaxed);
        // Ignore send errors: the event loop may have exited during shutdown,
        // in which case the count update is irrelevant.
        let _ = self.proxy.send_event(TrayEvent::RpcStateChanged);
    }

    /// Companion to [`Self::rpc_started`]. Every increment has exactly one
    /// matching decrement in the metrics layer, so underflow is impossible
    /// under normal operation.
    pub fn rpc_completed(&self) {
        self.active_rpcs.fetch_sub(1, Ordering::Relaxed);
        let _ = self.proxy.send_event(TrayEvent::RpcStateChanged);
    }
}

/// Tray runtime state. Constructed inside the event loop's `Init` callback
/// because creating the icon before the loop is actually running produces
/// stale icons on macOS (upstream issue tauri-apps/tray-icon#90).
///
/// Not `Send` — `TrayIcon` holds platform handles (`NSStatusItem` on macOS,
/// HWND on Windows) that are tied to the thread that created them.
struct TrayState {
    _tray: tray_icon::TrayIcon,
    status_item: MenuItem,
    ui_binary: Option<PathBuf>,
    /// Spawned UI child, retained so its pipes stay attached.
    ui_child: Option<Child>,
    open_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
}

/// Build the tray menu. Status starts at "0 active calls" because the daemon
/// hasn't accepted any RPCs yet at the point the tray comes up.
fn build_menu() -> Result<(
    Menu,
    MenuItem,
    tray_icon::menu::MenuId,
    tray_icon::menu::MenuId,
)> {
    let menu = Menu::new();
    let open = MenuItem::new("Open NodeSpace", true, None);
    let status = MenuItem::new("Status: 0 active calls", false, None);
    let quit = MenuItem::new("Quit", true, None);

    menu.append(&open).context("append Open item")?;
    menu.append(&PredefinedMenuItem::separator())
        .context("append separator")?;
    menu.append(&status).context("append Status item")?;
    menu.append(&PredefinedMenuItem::separator())
        .context("append separator")?;
    menu.append(&quit).context("append Quit item")?;

    Ok((menu, status, open.id().clone(), quit.id().clone()))
}

fn load_icon() -> Result<Icon> {
    let image = image::load_from_memory(TRAY_ICON_BYTES)
        .context("decode embedded tray icon")?
        .into_rgba8();
    let (w, h) = image.dimensions();
    Icon::from_rgba(image.into_raw(), w, h).context("build tray Icon from RGBA buffer")
}

/// Resolve the Tauri UI binary path. Honors `NODESPACE_UI_BINARY` so dev
/// builds and packaged installs can point at different artifacts without
/// recompiling. Returns `None` if unset — in that case "Open NodeSpace" logs
/// a warning and is otherwise inert, which is the right behavior in tests
/// and headless daemon runs.
fn resolve_ui_binary() -> Option<PathBuf> {
    std::env::var_os("NODESPACE_UI_BINARY").map(PathBuf::from)
}

/// Run the tray on the calling thread. **Must be the main thread on macOS.**
///
/// `seed_controller` is invoked synchronously *before* the event loop starts,
/// giving the caller a handle they can hand to the gRPC server (which runs
/// on a separate runtime). The value returned by `seed_controller` is handed
/// back from `run` once "Quit" is selected, so the caller can await any
/// resources it created at seed time (e.g. a gRPC `JoinHandle`).
///
/// Uses `event_loop.run_return` rather than `event_loop.run`: tao's `run`
/// calls `process::exit(0)` on macOS at `ControlFlow::Exit`, which would
/// kill the daemon before the gRPC server finishes draining. `run_return`'s
/// documented caveat (it may not return mid-window-resize) doesn't apply —
/// the daemon has no window, only a tray icon.
pub fn run<T>(seed_controller: impl FnOnce(TrayController) -> T) -> Result<T> {
    use tao::platform::run_return::EventLoopExtRunReturn;

    let mut event_loop: EventLoop<TrayEvent> = EventLoopBuilder::with_user_event().build();

    // Hide from the macOS dock and app switcher — nodespaced is a background
    // agent, not a foreground app. Must be set before the event loop starts.
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    let proxy = event_loop.create_proxy();

    // Forward muda's global menu channel into our tao loop. Without this the
    // menu clicks are queued in `MenuEvent::receiver()` and never observed.
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(TrayEvent::Menu(event));
    }));

    let active_rpcs = Arc::new(AtomicUsize::new(0));
    let quit_notify = Arc::new(tokio::sync::Notify::new());

    let seeded = seed_controller(TrayController {
        proxy: proxy.clone(),
        quit_notify: quit_notify.clone(),
        active_rpcs: active_rpcs.clone(),
    });

    let ui_binary = resolve_ui_binary();
    let mut state: Option<TrayState> = None;

    event_loop.run_return(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(tao::event::StartCause::Init) => {
                match initialize_tray(ui_binary.clone()) {
                    Ok(s) => state = Some(s),
                    Err(e) => {
                        tracing::error!(
                            error = ?e,
                            "Failed to initialize system tray; daemon will run without tray"
                        );
                        // Don't exit the loop — gRPC is still serving. The
                        // user can shut down via SIGTERM as before.
                    }
                }
            }

            Event::UserEvent(TrayEvent::Menu(menu_event)) => {
                let Some(s) = state.as_mut() else { return };
                if menu_event.id == s.open_id {
                    if let Err(e) = s.open_ui() {
                        tracing::error!(error = ?e, "Failed to spawn UI binary");
                    }
                } else if menu_event.id == s.quit_id {
                    tracing::info!("Tray Quit selected — initiating shutdown");
                    // `notify_waiters` wakes only currently-registered waiters.
                    // The gRPC server's `shutdown().await` is registered at
                    // server-build time (synchronously inside the seed closure
                    // above), so it's guaranteed to be parked here before the
                    // user can click Quit. New consumers of `shutdown()` must
                    // be registered with the same lifetime discipline.
                    quit_notify.notify_waiters();
                    *control_flow = ControlFlow::Exit;
                }
            }

            Event::UserEvent(TrayEvent::RpcStateChanged) => {
                if let Some(s) = state.as_ref() {
                    let count = active_rpcs.load(Ordering::Relaxed);
                    s.status_item
                        .set_text(format!("Status: {count} active calls"));
                }
            }

            _ => {}
        }
    });

    Ok(seeded)
}

fn initialize_tray(ui_binary: Option<PathBuf>) -> Result<TrayState> {
    let icon = load_icon()?;
    let (menu, status_item, open_id, quit_id) = build_menu()?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("NodeSpace")
        .with_icon(icon)
        .build()
        .context("build TrayIcon")?;

    Ok(TrayState {
        _tray: tray,
        status_item,
        ui_binary,
        ui_child: None,
        open_id,
        quit_id,
    })
}

impl TrayState {
    /// Spawn the Tauri UI binary, or — if a previous spawn is still alive —
    /// leave it alone and rely on the OS to focus an existing window.
    ///
    /// True cross-process window focus needs platform-specific calls
    /// (`NSRunningApplication::activate` etc.) and a focus signal over gRPC
    /// to the UI; both are tracked separately. For now "open if absent" is
    /// the smallest correct behavior.
    fn open_ui(&mut self) -> Result<()> {
        let Some(path) = self.ui_binary.as_ref() else {
            tracing::warn!(
                "Open NodeSpace selected but NODESPACE_UI_BINARY is unset; \
                 ignoring (set the env var or wire installation defaults)"
            );
            return Ok(());
        };

        // Reap any exited child first so a closed-then-reopened window works.
        if let Some(existing) = self.ui_child.as_mut() {
            match existing.try_wait() {
                Ok(Some(_status)) => {
                    self.ui_child = None;
                }
                Ok(None) => {
                    tracing::info!("UI binary already running; leaving it to OS to focus");
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(error = %e, "try_wait on UI child failed; respawning anyway");
                    self.ui_child = None;
                }
            }
        }

        let child = Command::new(path)
            .spawn()
            .with_context(|| format!("spawn UI binary {}", path.display()))?;
        self.ui_child = Some(child);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_decodes() {
        // Catches the common breakage where someone replaces the icon with a
        // non-PNG or a zero-byte file: `load_icon` exists precisely to bail
        // out before the event loop swallows the failure.
        let icon = load_icon().expect("embedded tray icon must decode");
        // Sanity check: tray-icon doesn't let us read back the size, but the
        // icon function would have errored on an empty rgba buffer.
        drop(icon);
    }

    // Both halves of the env-var contract live in one test: parallel tests
    // share the process env, so a separate "unset" test would race with the
    // "set" test and flake.
    #[test]
    fn resolve_ui_binary_honors_env_var() {
        std::env::set_var("NODESPACE_UI_BINARY", "/opt/nodespace/ui");
        let set_result = resolve_ui_binary();
        std::env::remove_var("NODESPACE_UI_BINARY");
        let unset_result = resolve_ui_binary();

        assert_eq!(
            set_result.as_deref(),
            Some(std::path::Path::new("/opt/nodespace/ui"))
        );
        assert!(unset_result.is_none());
    }
}

/// `tower::Layer` that bumps the tray's "active calls" counter for the
/// duration of every RPC. Wrapping the gRPC service this way means the
/// service implementations don't need to know the tray exists.
pub mod layer {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use tower::{Layer, Service};

    use super::TrayController;

    #[derive(Clone)]
    pub struct TrayMetricsLayer {
        controller: TrayController,
    }

    impl TrayMetricsLayer {
        pub fn new(controller: TrayController) -> Self {
            Self { controller }
        }
    }

    impl<S> Layer<S> for TrayMetricsLayer {
        type Service = TrayMetrics<S>;

        fn layer(&self, inner: S) -> Self::Service {
            TrayMetrics {
                inner,
                controller: self.controller.clone(),
            }
        }
    }

    #[derive(Clone)]
    pub struct TrayMetrics<S> {
        inner: S,
        controller: TrayController,
    }

    impl<S, Req> Service<Req> for TrayMetrics<S>
    where
        S: Service<Req> + Clone + Send + 'static,
        S::Future: Send + 'static,
        Req: Send + 'static,
    {
        type Response = S::Response;
        type Error = S::Error;
        type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

        fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.inner.poll_ready(cx)
        }

        fn call(&mut self, req: Req) -> Self::Future {
            self.controller.rpc_started();
            // tower's contract: `call` may be invoked again before the
            // previous future resolves, so move the readied service into the
            // future and leave a fresh clone in `self.inner`. We clone first
            // (a separate binding) to avoid an immutable + mutable borrow.
            let clone = self.inner.clone();
            let mut inner = std::mem::replace(&mut self.inner, clone);
            let controller = self.controller.clone();
            Box::pin(async move {
                let result = inner.call(req).await;
                controller.rpc_completed();
                result
            })
        }
    }
}
