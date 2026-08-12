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
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIconBuilder};

use crate::services::database_manager::{DatabaseId, DatabaseStatus, RegistrySnapshot};

/// PNG used for the menu-bar icon. 32×32 is large enough that macOS, Windows
/// and Linux all downscale gracefully; we keep one asset rather than shipping
/// a per-platform set since the daemon's footprint should stay small.
const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/tray-icon.png");

/// Set on the spawned UI process when the user opens a specific database from
/// the tray. The app treats it as the highest-precedence choice for that
/// launch only; a plain "Open NodeSpace" leaves it unset so the app restores
/// whatever it last had.
pub const INITIAL_DATABASE_ENV: &str = "NODESPACE_INITIAL_DATABASE";

/// Events the tonic side of the daemon can push into the tray event loop.
///
/// `MenuEvent` is forwarded verbatim from `tray-icon`'s global channel so the
/// `tao` loop can process menu clicks. `RpcStateChanged` is how the gRPC
/// layer reports activity for the live Status label.
enum TrayEvent {
    Menu(MenuEvent),
    RpcStateChanged,
    /// A fresh view of the database registry, rendered into the Databases
    /// submenu. Carries owned data because it crosses from the tokio runtime
    /// into the `!Send` tray loop.
    DatabasesChanged(Box<RegistrySnapshot>),
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

    /// Publish the current database registry to the tray so the Databases
    /// submenu can be (re)rendered.
    ///
    /// The registry lives behind the gRPC runtime and is built well after the
    /// tray loop starts, so it cannot be handed over at seed time — it arrives
    /// here instead. A snapshot that lands before the tray finishes
    /// initializing is retained and applied once it does, so ordering between
    /// the two does not matter.
    pub fn databases_changed(&self, snapshot: RegistrySnapshot) {
        // Ignore send errors: the loop may have exited during shutdown.
        let _ = self
            .proxy
            .send_event(TrayEvent::DatabasesChanged(Box::new(snapshot)));
    }
}

/// One database as rendered in the tray submenu.
///
/// Kept separate from the menu objects so the labelling rules — which is
/// open, which syncs, which cannot be opened — are decided in a plain
/// function that tests can call without a display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DatabaseMenuEntry {
    pub id: DatabaseId,
    pub label: String,
    /// False for a registry entry whose file is gone — shown, but not
    /// selectable, so the menu still explains why it can't be opened.
    pub enabled: bool,
}

/// Render a registry snapshot into tray entries, in registry order.
///
/// The label carries the two facts the menu is for: whether the database is
/// currently open, and whether it syncs to a cloud tenant. A missing file
/// replaces both, since neither is meaningful once the file is gone.
pub(crate) fn database_menu_entries(snapshot: &RegistrySnapshot) -> Vec<DatabaseMenuEntry> {
    snapshot
        .databases
        .iter()
        .map(|listing| {
            let mut markers: Vec<&str> = Vec::new();
            let missing = listing.status == DatabaseStatus::Missing;
            if missing {
                markers.push("missing");
            } else {
                if listing.status == DatabaseStatus::Open {
                    markers.push("open");
                }
                if listing.entry.bound_tenant_schema.is_some() {
                    markers.push("synced");
                }
            }

            let label = if markers.is_empty() {
                listing.entry.name.clone()
            } else {
                format!("{} — {}", listing.entry.name, markers.join(" · "))
            };

            DatabaseMenuEntry {
                id: listing.entry.id.clone(),
                label,
                enabled: !missing,
            }
        })
        .collect()
}

/// Tray runtime state. Constructed inside the event loop's `Init` callback
/// because creating the icon before the loop is actually running produces
/// stale icons on macOS (a known upstream tauri-apps/tray-icon bug).
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
    /// Databases submenu, repopulated whenever a registry snapshot arrives.
    databases_menu: Submenu,
    /// Menu items currently in that submenu, retained so they can be removed
    /// on the next rebuild, paired with the database each one opens.
    database_items: Vec<(MenuItem, DatabaseId)>,
}

/// Build the tray menu. Status starts at "0 active calls" because the daemon
/// hasn't accepted any RPCs yet at the point the tray comes up.
fn build_menu() -> Result<(
    Menu,
    MenuItem,
    Submenu,
    tray_icon::menu::MenuId,
    tray_icon::menu::MenuId,
)> {
    let menu = Menu::new();
    let open = MenuItem::new("Open NodeSpace", true, None);
    // Starts empty and disabled; the first registry snapshot fills it. The
    // daemon builds the registry after the tray is already up, so an empty
    // submenu is the honest state until then rather than a missing one.
    let databases = Submenu::new("Databases", false);
    let status = MenuItem::new("Status: 0 active calls", false, None);
    let quit = MenuItem::new("Quit", true, None);

    menu.append(&open).context("append Open item")?;
    menu.append(&databases)
        .context("append Databases submenu")?;
    menu.append(&PredefinedMenuItem::separator())
        .context("append separator")?;
    menu.append(&status).context("append Status item")?;
    menu.append(&PredefinedMenuItem::separator())
        .context("append separator")?;
    menu.append(&quit).context("append Quit item")?;

    Ok((
        menu,
        status,
        databases,
        open.id().clone(),
        quit.id().clone(),
    ))
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
    // The registry is built after the tray loop starts, so a snapshot can
    // arrive before `Init`. Hold the most recent one and apply it as soon as
    // there is a tray to apply it to.
    let mut pending_databases: Option<RegistrySnapshot> = None;

    event_loop.run_return(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(tao::event::StartCause::Init) => {
                match initialize_tray(ui_binary.clone()) {
                    Ok(mut s) => {
                        if let Some(snapshot) = pending_databases.take() {
                            if let Err(e) = s.rebuild_databases_menu(&snapshot) {
                                tracing::error!(error = ?e, "Failed to render Databases submenu");
                            }
                        }
                        state = Some(s)
                    }
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
                    if let Err(e) = s.open_ui(None) {
                        tracing::error!(error = ?e, "Failed to spawn UI binary");
                    }
                } else if let Some(database) = s.database_for_menu_id(&menu_event.id) {
                    tracing::info!(database = %database.as_str(), "Tray: opening UI on database");
                    if let Err(e) = s.open_ui(Some(database)) {
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

            Event::UserEvent(TrayEvent::DatabasesChanged(snapshot)) => match state.as_mut() {
                Some(s) => {
                    if let Err(e) = s.rebuild_databases_menu(&snapshot) {
                        tracing::error!(error = ?e, "Failed to render Databases submenu");
                    }
                }
                // Tray not up yet (or failed to initialize) — keep the latest.
                None => pending_databases = Some(*snapshot),
            },

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
    let (menu, status_item, databases_menu, open_id, quit_id) = build_menu()?;
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
        databases_menu,
        database_items: Vec::new(),
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
    /// Replace the Databases submenu contents with `snapshot`.
    ///
    /// Removes the previously-appended items rather than the whole submenu, so
    /// the submenu's own handle (held by the live menu) stays valid.
    fn rebuild_databases_menu(&mut self, snapshot: &RegistrySnapshot) -> Result<()> {
        for (item, _) in self.database_items.drain(..) {
            self.databases_menu
                .remove(&item)
                .context("remove stale database item")?;
        }

        let entries = database_menu_entries(snapshot);
        for entry in entries {
            let item = MenuItem::new(&entry.label, entry.enabled, None);
            self.databases_menu
                .append(&item)
                .context("append database item")?;
            self.database_items.push((item, entry.id));
        }

        // A submenu with nothing in it is unhelpful to open; disable it until
        // there is something to show.
        self.databases_menu
            .set_enabled(!self.database_items.is_empty());
        Ok(())
    }

    /// The database a menu id belongs to, if it is one of ours.
    fn database_for_menu_id(&self, id: &tray_icon::menu::MenuId) -> Option<DatabaseId> {
        self.database_items
            .iter()
            .find(|(item, _)| item.id() == id)
            .map(|(_, db)| db.clone())
    }

    fn open_ui(&mut self, database: Option<DatabaseId>) -> Result<()> {
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
                    // A running UI cannot be re-pointed from here: the initial
                    // database is read at startup. Switching an open window
                    // needs a focus/select signal over gRPC, tracked separately.
                    tracing::info!(
                        requested_database = database.as_ref().map(|d| d.as_str()),
                        "UI binary already running; leaving it to OS to focus"
                    );
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(error = %e, "try_wait on UI child failed; respawning anyway");
                    self.ui_child = None;
                }
            }
        }

        let mut command = Command::new(path);
        if let Some(id) = database.as_ref() {
            // Only set when the user picked a specific database, so a plain
            // "Open NodeSpace" still honours whatever the app last remembered.
            command.env(INITIAL_DATABASE_ENV, id.as_str());
        }
        let child = command
            .spawn()
            .with_context(|| format!("spawn UI binary {}", path.display()))?;
        self.ui_child = Some(child);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database_manager::{DatabaseEntry, DatabaseListing};

    fn listing(name: &str, status: DatabaseStatus, tenant: Option<&str>) -> DatabaseListing {
        DatabaseListing {
            entry: DatabaseEntry {
                id: DatabaseId::from(format!("id-{name}")),
                name: name.to_string(),
                path: PathBuf::from(format!("/tmp/{name}.db")),
                created_at: chrono::Utc::now(),
                last_opened_at: None,
                bound_tenant_schema: tenant.map(str::to_string),
                bound_tenant_collection: None,
            },
            status,
            is_default: false,
        }
    }

    fn snapshot(databases: Vec<DatabaseListing>) -> RegistrySnapshot {
        RegistrySnapshot {
            databases,
            default_database: None,
        }
    }

    /// The two facts the submenu exists to convey — open, and syncing — with a
    /// plain name when neither applies.
    #[test]
    fn labels_carry_open_and_synced_state() {
        let entries = database_menu_entries(&snapshot(vec![
            listing("Both", DatabaseStatus::Open, Some("tenant_demo")),
            listing("OpenOnly", DatabaseStatus::Open, None),
            listing("SyncedOnly", DatabaseStatus::Closed, Some("tenant_demo")),
            listing("Plain", DatabaseStatus::Closed, None),
        ]));

        let labels: Vec<&str> = entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Both — open · synced",
                "OpenOnly — open",
                "SyncedOnly — synced",
                "Plain",
            ]
        );
        assert!(entries.iter().all(|e| e.enabled));
    }

    /// A registry entry whose file is gone is still listed — silently dropping it
    /// would leave the user wondering where the database went — but it cannot be
    /// opened, and neither open nor synced is meaningful for it.
    #[test]
    fn missing_database_is_shown_but_not_selectable() {
        let entries = database_menu_entries(&snapshot(vec![listing(
            "Gone",
            DatabaseStatus::Missing,
            Some("tenant_demo"),
        )]));

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "Gone — missing");
        assert!(!entries[0].enabled);
    }

    /// Registry order is preserved and every entry keeps its own id, so a click
    /// opens the database the user actually picked.
    #[test]
    fn entries_keep_registry_order_and_ids() {
        let entries = database_menu_entries(&snapshot(vec![
            listing("First", DatabaseStatus::Closed, None),
            listing("Second", DatabaseStatus::Closed, None),
        ]));

        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["id-First", "id-Second"]);
    }

    #[test]
    fn empty_registry_renders_no_entries() {
        assert!(database_menu_entries(&snapshot(vec![])).is_empty());
    }

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
