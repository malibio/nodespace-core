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

use std::path::{Path, PathBuf};
use std::process::Command;
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
///
/// Read at startup by a *cold* launch (no other instance running). When an
/// instance is already running, the environment of a freshly-spawned process
/// is invisible to it — the same database id is also passed as a `--database
/// <id>` CLI argument (see [`TrayState::open_ui`]), which is what the app's
/// `tauri-plugin-single-instance` relaunch handler reads instead.
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
    /// The gRPC task has stopped running for a reason other than the tray's
    /// own "Quit" menu item -- an OS signal (SIGTERM/SIGINT) drained it, or
    /// it returned an error, or it panicked. Nothing else watches that task,
    /// so without this the tao loop has no way to learn its work is done and
    /// sits forever with a live tray icon fronting a dead gRPC server.
    GrpcTaskFinished,
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

    /// Tell the tray loop the gRPC task is done, so it exits too instead of
    /// leaving a tray icon fronting a dead server. Call this from whatever is
    /// watching the gRPC task's `JoinHandle` once it resolves -- by a signal
    /// draining it, by it returning an error, or by it panicking -- covering
    /// every way the task can stop other than the tray's own "Quit" click,
    /// which already reaches `ControlFlow::Exit` through the menu handler.
    pub fn grpc_task_finished(&self) {
        let _ = self.proxy.send_event(TrayEvent::GrpcTaskFinished);
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
/// The label carries the facts the registry persists — which database is the
/// default, and which sync to a cloud tenant. Those change only when the user
/// explicitly changes them, so a menu rendered once survives them better than
/// it would runtime state. Deliberately NOT whether a database is currently
/// *open*: the idle reaper closes databases minutes into
/// normal use, and this menu is only refreshed when the daemon pushes a new
/// snapshot, so an open marker would be confidently wrong most of the time. A
/// live open indicator belongs with live refresh, tracked separately.
///
/// A missing file replaces the other markers, since neither is meaningful once
/// the file is gone.
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
                if listing.is_default {
                    markers.push("default");
                }
                // Safe to show only because the submenu is now rebuilt on every
                // registry/open-set change. Pushed once at boot it would be
                // confidently wrong within minutes: the idle reaper closes
                // databases and in-app switching opens others.
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
                            s.rebuild_databases_menu(&snapshot);
                        }
                        state = Some(s);
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
                Some(s) => s.rebuild_databases_menu(&snapshot),
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

            Event::UserEvent(TrayEvent::GrpcTaskFinished) => {
                // The gRPC task is already stopped by the time this arrives
                // -- unlike the Quit branch above, there's nothing left to
                // wake via `quit_notify`, just the loop itself to exit.
                tracing::info!("gRPC task finished outside of tray Quit — shutting down tray loop");
                *control_flow = ControlFlow::Exit;
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
        open_id,
        quit_id,
        databases_menu,
        database_items: Vec::new(),
    })
}

impl TrayState {
    /// Replace the Databases submenu contents with `snapshot`.
    ///
    /// Removes the previously-appended items rather than the whole submenu, so
    /// the submenu's own handle (held by the live menu) stays valid.
    fn rebuild_databases_menu(&mut self, snapshot: &RegistrySnapshot) {
        // Clear the tracking list unconditionally, whatever removal reports: an
        // item left in the native submenu but dropped from the list is
        // unreachable (clicks stop resolving) AND gets a duplicate appended
        // beside it on the next rebuild. Forgetting one is the worse failure,
        // so log and keep going rather than bail mid-rebuild.
        for (item, _) in self.database_items.iter() {
            if let Err(e) = self.databases_menu.remove(item) {
                tracing::warn!(error = ?e, "Failed to remove a stale database menu item");
            }
        }
        self.database_items.clear();

        let mut failed = 0usize;
        for entry in database_menu_entries(snapshot) {
            let item = MenuItem::new(&entry.label, entry.enabled, None);
            match self.databases_menu.append(&item) {
                Ok(()) => self.database_items.push((item, entry.id)),
                Err(e) => {
                    failed += 1;
                    tracing::warn!(error = ?e, "Failed to append a database menu item");
                }
            }
        }
        if failed > 0 {
            tracing::warn!(failed, "Databases submenu is incomplete");
        }

        // Computed on the way out, not only on the success path, so a partial
        // rebuild never leaves reachable items behind a greyed-out parent.
        self.databases_menu
            .set_enabled(!self.database_items.is_empty());
    }

    /// The database a menu id belongs to, if it is one of ours.
    fn database_for_menu_id(&self, id: &tray_icon::menu::MenuId) -> Option<DatabaseId> {
        self.database_items
            .iter()
            .find(|(item, _)| item.id() == id)
            .map(|(_, db)| db.clone())
    }

    /// Spawn the Tauri UI binary, optionally pointing it at `database`.
    ///
    /// Always spawns — the daemon has no reliable way to know whether a UI
    /// process is already alive: a child it spawned itself may have exited
    /// without the tray noticing, and one launched outside the tray (Dock
    /// icon, Spotlight, a previous run) leaves no child here to check at all.
    /// Rather than track liveness in the daemon, dedup is delegated to the app
    /// itself via `tauri-plugin-single-instance`: a genuinely-second launch
    /// detects the running instance, forwards its argv to it, and exits
    /// immediately, so a real UI process almost never survives alongside an
    /// existing one regardless of how many times this is called or how the
    /// first instance was started. Not an absolute guarantee on every
    /// platform: the plugin's macOS backend claims the single-instance lock
    /// with a connect-then-bind probe rather than one atomic OS primitive (Windows
    /// uses a named mutex, Linux a D-Bus name claim — both atomic), so two
    /// processes launched within that probe's scheduling gap can theoretically
    /// both see "nobody's home" and both survive. Narrow and outside this
    /// crate's control; documented here so it isn't mistaken for a promise
    /// this code makes.
    ///
    /// The requested database is passed two ways so either launch path can
    /// read it: as `INITIAL_DATABASE_ENV`, which a *cold* launch (no other
    /// instance running) reads at startup, and as a `--database <id>` CLI
    /// argument, which is what `tauri-plugin-single-instance` forwards to an
    /// *already-running* instance's relaunch handler — that handler has no
    /// visibility into the new process's environment, only its argv. The
    /// already-running instance focuses its window and switches to the
    /// requested database in place on receiving it.
    fn open_ui(&mut self, database: Option<DatabaseId>) -> Result<()> {
        let Some(path) = self.ui_binary.as_ref() else {
            tracing::warn!(
                "Open NodeSpace selected but NODESPACE_UI_BINARY is unset; \
                 ignoring (set the env var or wire installation defaults)"
            );
            return Ok(());
        };

        let mut command = build_ui_command(path, database.as_ref());
        let mut child = command
            .spawn()
            .with_context(|| format!("spawn UI binary {}", path.display()))?;

        // Reap on a plain OS thread rather than the tao event loop: a relaunch
        // that the app's own single-instance guard dedupes exits almost
        // immediately, and nothing else in this process ever calls `wait` on
        // it, so an unreaped child would sit as a zombie for the rest of the
        // daemon's lifetime (its parent). `Child` is `Send`, so this never
        // blocks tray event handling.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

/// Build the command to launch the UI binary, optionally requesting it open on
/// `database`. Split out from [`TrayState::open_ui`] so the exact args/env are
/// assertable in tests without actually spawning a process.
fn build_ui_command(path: &Path, database: Option<&DatabaseId>) -> Command {
    let mut command = Command::new(path);
    if let Some(id) = database {
        // Only set when the user picked a specific database, so a plain
        // "Open NodeSpace" still honours whatever the app last remembered.
        command.env(INITIAL_DATABASE_ENV, id.as_str());
        command.arg("--database").arg(id.as_str());
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database_manager::{DatabaseEntry, DatabaseListing};

    fn listing_with_default(
        name: &str,
        status: DatabaseStatus,
        tenant: Option<&str>,
        is_default: bool,
    ) -> DatabaseListing {
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
            is_default,
        }
    }

    fn listing(name: &str, status: DatabaseStatus, tenant: Option<&str>) -> DatabaseListing {
        listing_with_default(name, status, tenant, false)
    }

    fn snapshot(databases: Vec<DatabaseListing>) -> RegistrySnapshot {
        RegistrySnapshot {
            databases,
            default_database: None,
        }
    }

    /// The two facts the submenu conveys — which database is the default, and
    /// which sync — with a plain name when neither applies.
    #[test]
    fn labels_carry_default_and_synced_state() {
        let entries = database_menu_entries(&snapshot(vec![
            listing_with_default("Both", DatabaseStatus::Closed, Some("tenant_demo"), true),
            listing_with_default("DefaultOnly", DatabaseStatus::Closed, None, true),
            listing("SyncedOnly", DatabaseStatus::Closed, Some("tenant_demo")),
            listing("Plain", DatabaseStatus::Closed, None),
        ]));

        let labels: Vec<&str> = entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Both — default · synced",
                "DefaultOnly — default",
                "SyncedOnly — synced",
                "Plain",
            ]
        );
        assert!(entries.iter().all(|e| e.enabled));
    }

    /// Which database is open now IS shown. It was omitted while the submenu was
    /// built once at boot, because the idle reaper closes databases and in-app
    /// switching opens others — a marker pushed once would be wrong within minutes
    /// of normal use. The menu is rebuilt on every registry/open-set change now,
    /// so the marker tracks reality.
    #[test]
    fn open_state_is_reflected_in_the_label() {
        let entries = database_menu_entries(&snapshot(vec![
            listing("Alpha", DatabaseStatus::Closed, None),
            listing("Beta", DatabaseStatus::Open, None),
        ]));

        assert_eq!(entries[0].label, "Alpha");
        assert_eq!(entries[1].label, "Beta — open");
    }

    /// Marker order is fixed so a label does not reshuffle between refreshes,
    /// which under live updates would read as flicker rather than information.
    #[test]
    fn markers_render_in_a_stable_order() {
        let entries = database_menu_entries(&snapshot(vec![listing_with_default(
            "All",
            DatabaseStatus::Open,
            Some("tenant_demo"),
            true,
        )]));

        assert_eq!(entries[0].label, "All — default · open · synced");
    }

    /// A missing entry never gains an open marker: its file is gone, so "open"
    /// would be nonsense even though the status enum can only hold one value.
    #[test]
    fn missing_beats_every_other_marker() {
        let entries = database_menu_entries(&snapshot(vec![listing_with_default(
            "Gone",
            DatabaseStatus::Missing,
            Some("tenant_demo"),
            true,
        )]));

        assert_eq!(entries[0].label, "Gone — missing");
        assert!(!entries[0].enabled);
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

    /// A plain "Open NodeSpace" (no database requested) must not set the env
    /// var or the CLI flag — otherwise a cold launch would misread it as a
    /// database pick and override whatever the app last remembered.
    #[test]
    fn build_ui_command_without_database_sets_neither_env_nor_arg() {
        let command = build_ui_command(Path::new("/opt/nodespace/ui"), None);

        assert!(command.get_envs().all(|(k, _)| k != INITIAL_DATABASE_ENV));
        assert!(command.get_args().next().is_none());
    }

    /// Picking a specific database must set both the env var (read by a cold
    /// launch at startup) and the `--database <id>` CLI flag (forwarded by
    /// `tauri-plugin-single-instance` to an already-running instance, which has
    /// no visibility into the new process's environment) — one code path
    /// handles both launch scenarios without the tray knowing which applies.
    #[test]
    fn build_ui_command_with_database_sets_env_and_cli_arg() {
        let id = DatabaseId::from("db-123".to_string());
        let command = build_ui_command(Path::new("/opt/nodespace/ui"), Some(&id));

        let env_value = command
            .get_envs()
            .find(|(k, _)| *k == INITIAL_DATABASE_ENV)
            .and_then(|(_, v)| v);
        assert_eq!(env_value, Some(std::ffi::OsStr::new("db-123")));

        let args: Vec<&std::ffi::OsStr> = command.get_args().collect();
        assert_eq!(args, vec!["--database", "db-123"]);
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
