// Tauri event channel constants for the agent subsystem (kept in desktop-app
// because they depend on Tauri, which is not a dependency of nodespace-agent).
pub mod agent_events;

// Tauri commands module (public for dev-server access)
pub mod commands;

// Local type mirrors for command layer (severs nodespace_core dep from commands/)
pub mod types;

// Application preferences management
pub mod preferences;

// Shared constants
pub mod constants;

// Background services
pub mod services;

// gRPC-backed node event watcher. Inert until activated —
// see watcher.rs module docs for activation gating.
pub mod watcher;

// Daemon lifecycle: launchd (macOS), systemd (Linux), direct spawn (Windows)
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub mod daemon_setup;

// First-launch skill installer
pub mod skill_setup;

// App update check (detect newer releases)
pub mod update_check;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn toggle_sidebar() -> String {
    "Sidebar toggled!".to_string()
}

/// True when the frontend debug channel is enabled (env `NS_FRONTEND_LOG` set to
/// a file path). The frontend gates its forwarding on this so normal builds pay
/// no IPC cost; diagnostics set the env per window to capture console messages,
/// invoke/network calls, DOM snapshots, and store dumps as structured NDJSON.
#[tauri::command]
fn frontend_log_enabled() -> bool {
    std::env::var_os("NS_FRONTEND_LOG").is_some()
}

/// Append one NDJSON debug-channel line to the file named by `NS_FRONTEND_LOG`.
/// `line` is a pre-serialized JSON object (see `DebugEvent` on the frontend) —
/// this command is a dumb sink with no schema knowledge of its contents.
/// No-op if the env var is unset. Best-effort (diagnostic only) — never fails
/// the caller.
#[tauri::command]
fn frontend_log(line: String) {
    if let Some(path) = std::env::var_os("NS_FRONTEND_LOG") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// Report the current daemon health to the frontend.
///
/// Returns "healthy", "starting", or "not_running". The frontend uses this
/// to decide whether to show an error state.
#[tauri::command]
async fn check_daemon_status() -> String {
    daemon_status_body().await
}

/// The body of [`check_daemon_status`], factored out and made `pub` so the
/// readiness integration test in `tests/` — a separate crate — can call the
/// exact same logic the Tauri command invokes, including its
/// `resolve_socket_path()` call. Kept out of the `#[tauri::command]`-
/// annotated function itself: that macro generates hidden crate-scoped
/// items keyed to the function's identifier, and marking the annotated
/// function `pub` collides with them (`E0255`, defined multiple times).
pub async fn daemon_status_body() -> String {
    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    {
        use daemon_setup::{check_daemon_socket, DaemonStatus};

        // Probe the SAME socket the gRPC client dials (honors NODESPACED_SOCKET).
        let socket_path = crate::services::grpc_client::resolve_socket_path();
        return match check_daemon_socket(socket_path.as_path()).await {
            DaemonStatus::Healthy => "healthy".to_string(),
            DaemonStatus::Starting => "starting".to_string(),
            DaemonStatus::NotRunning => "not_running".to_string(),
        };
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    "healthy".to_string()
}

// Include test module
#[cfg(test)]
mod tests;

// Regression coverage for the CloseRequested-veto shutdown race — its own
// module (rather than folded into `tests`) since it needs `tauri::test`'s
// MockRuntime to drive real RunEvent delivery, not just plain unit tests.
#[cfg(test)]
mod shutdown_tests;

/// Shared shutdown token for graceful background task termination.
///
/// Managed as Tauri state so it can be accessed from both the setup phase
/// (where background tasks are spawned) and the run event handler (where
/// shutdown is triggered). When cancelled, all background tasks exit their
/// loops before the Tokio runtime drops.
#[derive(Clone)]
pub struct ShutdownToken {
    cancellation: tokio_util::sync::CancellationToken,
    /// Guards the one-shot shutdown sequence in [`graceful_shutdown`]. Lives
    /// on the token itself rather than a function-local `static`: a `static`
    /// is process-global, so it would (a) leak across every test in this
    /// binary instead of resetting per `ShutdownToken` instance, and (b) — for
    /// the window-per-database work — trip on the first window's close and
    /// silently no-op every other window's teardown. One token per app today
    /// still means one guard per app; this just puts it in a place that
    /// scales to one-per-window later without another rewrite.
    shutdown_started: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ShutdownToken {
    fn new() -> Self {
        Self {
            cancellation: tokio_util::sync::CancellationToken::new(),
            shutdown_started: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Create a child token for a background task.
    /// Cancelling the parent automatically cancels all children.
    pub fn child_token(&self) -> tokio_util::sync::CancellationToken {
        self.cancellation.child_token()
    }

    /// Signal all background tasks to shut down.
    /// Idempotent - safe to call multiple times.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// True once [`Self::cancel`] has run.
    #[cfg(test)]
    fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Marks the one-shot shutdown sequence as started. Returns `true` the
    /// first time it's called on this token, `false` on every later call —
    /// [`graceful_shutdown`] uses this to run its teardown exactly once even
    /// though both `ExitRequested` and `Exit` drive it.
    fn begin_shutdown_once(&self) -> bool {
        !self
            .shutdown_started
            .swap(true, std::sync::atomic::Ordering::SeqCst)
    }
}

/// A database id from a relaunch that arrived before the frontend had
/// registered its `tray:select-database` listener.
///
/// Tauri does not buffer or replay an event emitted while it has zero current
/// listeners (confirmed against the `tauri` event-emission source — it looks
/// up registered handlers for the event name at the instant `emit` runs and
/// nothing more), and the listener only exists once `app-shell.svelte` has
/// mounted and its `listen()` IPC round-trip has resolved — a relaunch in that
/// narrow window (webview still booting) would otherwise focus the window but
/// silently drop the switch. `handle_relaunch` stashes here in addition to
/// emitting; the frontend pulls it once via
/// [`take_pending_tray_database_selection`] right after registering the
/// listener, so the switch lands either way `switchTo` is idempotent against a
/// value the live event also happened to deliver.
///
/// A plain module-level `Mutex` rather than Tauri-managed state: the
/// single-instance plugin's relaunch callback can fire before this app's own
/// `app.manage(..)` calls inside `.setup()` have run (both start during
/// `Builder::run`, and the plugin is deliberately registered first), so
/// managed state would need its own readiness check; a `static` has none.
static PENDING_TRAY_DATABASE_SELECTION: std::sync::Mutex<Option<String>> =
    std::sync::Mutex::new(None);

/// One-shot read of a database id [`handle_relaunch`] stashed before the
/// frontend's `tray:select-database` listener existed to receive the live
/// event. Consumes the value so a later, unrelated call never re-applies a
/// stale selection.
#[tauri::command]
fn take_pending_tray_database_selection() -> Option<String> {
    PENDING_TRAY_DATABASE_SELECTION.lock().unwrap().take()
}

/// Handle a relaunch forwarded by `tauri-plugin-single-instance`: the daemon's
/// tray always spawns a fresh UI process (it cannot reliably tell whether one
/// is already alive), so every click after the first arrives here instead of
/// as a second window.
///
/// Always focuses the running window — cross-process activation
/// (`NSRunningApplication::activate` on macOS, the equivalent on Windows/Linux)
/// is exactly what `WebviewWindow::set_focus` does under the hood, so a plain
/// "Open NodeSpace" click while the app is already running now does something:
/// it raises the window instead of silently no-op'ing. If the relaunch also
/// named a specific database (`--database <id>`, set by the tray only when the
/// user picked one from the submenu), forward it to the frontend — both as a
/// live event and stashed for the pull-based fallback above — so it can
/// switch in place via `databaseStore.switchTo` — the same path an in-app
/// switcher click already uses.
fn handle_relaunch<R: tauri::Runtime>(app: &tauri::AppHandle<R>, argv: &[String]) {
    use tauri::{Emitter, Manager};

    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!("single-instance relaunch: no main window to focus");
        return;
    };
    if let Err(e) = window.unminimize() {
        tracing::warn!(error = %e, "failed to unminimize the main window on relaunch");
    }
    if let Err(e) = window.show() {
        tracing::warn!(error = %e, "failed to show the main window on relaunch");
    }
    if let Err(e) = window.set_focus() {
        tracing::warn!(error = %e, "failed to focus the main window on relaunch");
    }

    if let Some(id) = parse_relaunch_database_arg(argv) {
        *PENDING_TRAY_DATABASE_SELECTION.lock().unwrap() = Some(id.clone());
        if let Err(e) = window.emit("tray:select-database", id) {
            tracing::warn!(error = %e, "failed to emit tray:select-database");
        }
    }
}

/// Extract the database id from a relaunch's argv, as passed by the daemon's
/// tray via `--database <id>` (the CLI counterpart of
/// `nodespace_daemon::tray::INITIAL_DATABASE_ENV`, forwarded here because the
/// environment of the just-exited relaunch process is not visible to the
/// already-running instance) when the user picks a specific database while
/// the app is already running. `None` when the relaunch carried no database
/// request (a plain "Open NodeSpace") or the flag's value is missing or blank.
fn parse_relaunch_database_arg(argv: &[String]) -> Option<String> {
    let value = argv
        .iter()
        .position(|a| a == "--database")
        .and_then(|i| argv.get(i + 1))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::{menu::*, Emitter, Manager};

    // Initialize tracing — respects RUST_LOG env var, defaults to info for nodespace_app
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("nodespace_app=info")),
        )
        .try_init()
        .ok();

    // Create shutdown token for coordinating graceful background task
    // termination. `app.run`'s event handler (`handle_run_event`) no longer
    // needs its own clone — it looks the token up via managed state instead,
    // the same way `graceful_shutdown` always has.
    let shutdown_token_for_setup = ShutdownToken::new();

    // Single-instance guard: the daemon's tray always spawns a fresh UI
    // process when a database is picked (see `nodespace_daemon::tray::
    // TrayState::open_ui`) — it has no reliable way to know whether a UI
    // process from an earlier launch is still alive, and none at all when the
    // app was started outside the tray. Rather than the daemon tracking
    // liveness, this plugin makes any second launch detect the running
    // instance, forward its argv to it, and exit immediately, so a real UI
    // process almost never survives alongside an existing one regardless of
    // launch path. Must be registered before any other plugin (upstream
    // requirement). Note this plugin's macOS backend claims the lock with a
    // connect-then-bind probe rather than one atomic OS primitive (unlike its
    // Windows/Linux backends), so it is not an absolute guarantee there — see
    // the doc comment on `TrayState::open_ui` in the daemon crate.
    let mut builder = tauri::Builder::default();
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            handle_relaunch(app, &argv);
        }));
    }

    let app = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            // Create menu items
            let toggle_sidebar = MenuItemBuilder::new("Toggle Sidebar")
                .id("toggle_sidebar")
                .accelerator("CmdOrCtrl+B")
                .build(app)?;

            let toggle_status_bar = MenuItemBuilder::new("Toggle Status Bar")
                .id("toggle_status_bar")
                .build(app)?;

            let quit = MenuItemBuilder::new("Quit")
                .id("quit")
                .accelerator("CmdOrCtrl+Q")
                .build(app)?;

            let import_folder = MenuItemBuilder::new("Import Folder...")
                .id("import_folder")
                .accelerator("CmdOrCtrl+Shift+I")
                .build(app)?;

            let open_settings = MenuItemBuilder::new("Settings...")
                .id("open_settings")
                .accelerator("CmdOrCtrl+,")
                .build(app)?;

            let open_integrations = MenuItemBuilder::new("Integrations...")
                .id("open_integrations")
                .build(app)?;

            let settings_separator = PredefinedMenuItem::separator(app)?;
            let integrations_separator = PredefinedMenuItem::separator(app)?;

            let import_submenu = SubmenuBuilder::new(app, "Import")
                .items(&[&import_folder])
                .build()?;

            // Standard Edit menu items for clipboard operations
            // These are required on macOS for Cmd+C/V/X to work in WebView
            let cut = PredefinedMenuItem::cut(app, Some("Cut"))?;
            let copy = PredefinedMenuItem::copy(app, Some("Copy"))?;
            let paste = PredefinedMenuItem::paste(app, Some("Paste"))?;
            let select_all = PredefinedMenuItem::select_all(app, Some("Select All"))?;
            let undo = PredefinedMenuItem::undo(app, Some("Undo"))?;
            let redo = PredefinedMenuItem::redo(app, Some("Redo"))?;

            // Create submenus
            // macOS app menu (first menu is always the app name on macOS)
            let app_menu = SubmenuBuilder::new(app, "NodeSpace")
                .items(&[&quit])
                .build()?;

            let file_menu = SubmenuBuilder::new(app, "File")
                .items(&[
                    &import_submenu,
                    &settings_separator,
                    &open_settings,
                    &integrations_separator,
                    &open_integrations,
                ])
                .build()?;

            // Edit menu with standard shortcuts (required for macOS WebView clipboard)
            let edit_menu = SubmenuBuilder::new(app, "Edit")
                .items(&[&undo, &redo, &cut, &copy, &paste, &select_all])
                .build()?;

            let view_menu = SubmenuBuilder::new(app, "View")
                .items(&[&toggle_sidebar, &toggle_status_bar])
                .build()?;

            // Create main menu
            let menu = MenuBuilder::new(app)
                .items(&[&app_menu, &file_menu, &edit_menu, &view_menu])
                .build()?;

            // Set the menu
            app.set_menu(menu)?;

            // Register shutdown token as managed state for background task coordination.
            app.manage(shutdown_token_for_setup.clone());

            // Kill the running daemon if its binary is stale (size mismatch vs bundled
            // sidecar). Must run before connect_lazy so the frontend never connects to
            // an outdated daemon. ensure_daemon_running (spawned below) then extracts
            // the fresh binary and restarts via launchd/systemd.
            #[cfg(unix)]
            crate::daemon_setup::kill_stale_daemon_sync(app);

            // Manage the gRPC client EAGERLY via a lazy channel (connects on first
            // RPC). Previously `manage(GrpcClient)` ran only after the async connect
            // below, so a frontend command issued at startup (e.g. the date page's
            // `get_children_tree`) raced it and got a fatal "state not managed for
            // field `client`", closing the view. With the client
            // managed up front, an early call instead yields a retryable transport
            // error until the daemon is reachable.
            #[cfg(any(unix, windows))]
            app.manage(crate::services::GrpcClient::connect_lazy());

            // Best-effort update check: on a background task so it never delays
            // startup, and it only emits when a strictly-newer release exists — no
            // event on "up to date" or any failure, so the frontend banner appears
            // solely on a real update. Independent of the daemon path below.
            {
                use tauri::{Emitter, Manager};

                let update_app_handle = app.handle().clone();
                let current_version = app.package_info().version.to_string();
                tauri::async_runtime::spawn(async move {
                    let status = update_check::check_for_update(&current_version).await;
                    if status.update_available {
                        if let Some(window) = update_app_handle.get_webview_window("main") {
                            let _ = window.emit(update_check::UPDATE_AVAILABLE_EVENT, &status);
                        }
                    }
                });
            }

            // Spawn async task to start the daemon (if needed) and wire up the
            // watcher, token stream, and Pro probe on the (already-managed) client.
            // setup() is synchronous so we can't block_on here — spawn a task instead.
            #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
            {
                use tauri::Emitter;

                let app_handle = app.handle().clone();
                let session_token = shutdown_token_for_setup.child_token();

                tauri::async_runtime::spawn(async move {
                    // Ensure nodespaced service is installed and running (launchd on macOS, systemd on Linux).
                    {
                        use daemon_setup::{ensure_daemon_running, DaemonStatus};

                        // Signal the frontend to hold off on gRPC calls until the daemon is ready.
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.emit("daemon-status", "starting");
                        }

                        match ensure_daemon_running(&app_handle).await {
                            Ok(DaemonStatus::Healthy) => {
                                tracing::info!("nodespaced is running");
                                if let Some(window) = app_handle.get_webview_window("main") {
                                    let _ = window.emit("daemon-status", "healthy");
                                }
                            }
                            Ok(status) => {
                                tracing::warn!("nodespaced not yet healthy: {:?}", status);
                                if let Some(window) = app_handle.get_webview_window("main") {
                                    let _ = window.emit("daemon-status", "not_running");
                                }
                            }
                            Err(e) => {
                                tracing::error!("Daemon setup failed: {:#}", e);
                                if let Some(window) = app_handle.get_webview_window("main") {
                                    let _ = window.emit("daemon-status", "not_running");
                                }
                            }
                        }
                    }

                    // First-launch: install NodeSpace skill into detected agents.
                    // Idempotent — no-op once ~/.nodespace/setup.json marks skill_installed.
                    {
                        use crate::skill_setup;
                        // WARN logging (once per new failure, DEBUG on repeats) happens
                        // inside install_skill itself — see its module docs. Emitting to
                        // the frontend only happens here: this is the one fire-and-forget
                        // call site with no other way to reach the user (the onboarding/
                        // manual-retry commands already return their result synchronously
                        // to an awaiting caller, so emitting there too would show the same
                        // failure twice).
                        let result = skill_setup::install_skill(false, &app_handle).await;
                        if result.success {
                            if !result.agents_installed.is_empty() {
                                tracing::info!(
                                    "NodeSpace skill installed into: {:?}",
                                    result.agents_installed
                                );
                            }
                            if let Some(warning) = result.cli_warning {
                                tracing::warn!("{}", warning);
                                if let Some(window) = app_handle.get_webview_window("main") {
                                    let _ = window.emit(
                                        "skill:cli-missing",
                                        serde_json::json!({ "warning": warning }),
                                    );
                                }
                            }
                        } else if result.failure_is_new {
                            if let (Some(window), Some(error)) =
                                (app_handle.get_webview_window("main"), result.error)
                            {
                                let _ = window.emit(
                                    "skill:install-failed",
                                    serde_json::json!({ "error": error }),
                                );
                            }
                        }
                    }

                    // The gRPC client is already managed (lazy) above. Fetch it and
                    // wire the watcher, token stream, and Pro probe; the underlying
                    // channel connects on first use.
                    use tauri::Manager;
                    let grpc_client = (*app_handle.state::<crate::services::GrpcClient>()).clone();
                    let channel = grpc_client.channel().await;
                    // The watcher rides the shared client so its WatchNodes stream
                    // targets the active database and re-subscribes on switch (ADR-053).
                    watcher::spawn(app_handle.clone(), grpc_client.clone(), session_token);
                    // Subscribe to token stream for ai-chat node inference events.
                    commands::local_agent::start_token_stream_subscription(
                        app_handle.clone(),
                        grpc_client,
                    );

                    // Pro capability probe: a single WatchSyncStatus call on the same
                    // channel. Community `nodespaced` returns `Status::Unimplemented`
                    // → tier=Community → sync pill stays hidden. Pro `nodespaced-pro`
                    // answers → tier=Pro → pill renders and listens for `sync:status`.
                    let pro = crate::services::ProClient::probe_on_channel(channel).await;
                    let tier = pro.tier().await;
                    let last_status = pro.last_status().await;
                    let payload = serde_json::json!({
                        "tier": tier,
                        "initial_status": last_status.as_ref().map(|s| {
                            serde_json::json!({
                                "state": s.state,
                                "detail": s.detail,
                                "user_email": s.user_email,
                            })
                        }),
                    });
                    app_handle.manage(pro);
                    if let Err(e) = app_handle.emit("pro:tier-detected", payload) {
                        tracing::warn!(error = %e, "failed to emit pro:tier-detected");
                    }
                    tracing::info!(?tier, "Pro capability probe done");

                    // Re-establish the "daemon unreachable" signal lost when the
                    // eager lazy client replaced the connect()-or-emit path. The lazy
                    // channel never fails at startup, so a genuinely-down daemon would
                    // otherwise leave the UI silently empty. After the probe has
                    // exercised the channel, confirm reachability with the same socket
                    // check `check_daemon_status` uses and emit `not_running` so
                    // app-shell shows its error banner + retry. `Starting`
                    // is transient — only `NotRunning` trips the banner.
                    {
                        use daemon_setup::{check_daemon_socket, DaemonStatus};
                        // Probe the SAME socket the gRPC client dials (honors
                        // NODESPACED_SOCKET), not the hardcoded default — else a
                        // socket override falsely reports the daemon down.
                        let socket_path = crate::services::grpc_client::resolve_socket_path();
                        if matches!(
                            check_daemon_socket(socket_path.as_path()).await,
                            DaemonStatus::NotRunning
                        ) {
                            tracing::error!("nodespaced unreachable after startup");
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.emit("daemon-status", "not_running");
                            }
                        }
                    }
                });
            }

            // Streaming task registry for PTY session cancellation
            app.manage(commands::agent_session::StreamingTaskRegistry::default());

            Ok(())
        })
        .on_menu_event(|app, event| {
            let toggle_sidebar_id = MenuId::new("toggle_sidebar");
            let toggle_status_bar_id = MenuId::new("toggle_status_bar");
            let quit_id = MenuId::new("quit");
            let import_folder_id = MenuId::new("import_folder");
            let open_settings_id = MenuId::new("open_settings");
            let open_integrations_id = MenuId::new("open_integrations");

            if *event.id() == toggle_sidebar_id {
                // Emit an event to the frontend
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("menu-toggle-sidebar", ());
                    println!("Sidebar toggle requested from menu");
                }
            } else if *event.id() == toggle_status_bar_id {
                // Emit an event to the frontend to toggle status bar
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("menu-toggle-status-bar", ());
                    println!("Status bar toggle requested from menu");
                }
            } else if *event.id() == import_folder_id {
                // Emit an event to the frontend to open import dialog
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("menu-import-folder", ());
                    println!("Import folder requested from menu");
                }
            } else if *event.id() == open_settings_id {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("menu-open-settings", ());
                }
            } else if *event.id() == open_integrations_id {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("menu-open-integrations", ());
                }
            } else if *event.id() == quit_id {
                // Request exit through Tauri's event loop instead of std::process::exit(0)
                // This triggers RunEvent::ExitRequested, allowing proper cleanup
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            toggle_sidebar,
            frontend_log_enabled,
            frontend_log,
            check_daemon_status,
            take_pending_tray_database_selection,
            update_check::check_for_update_command,
            commands::pro_sync::pro_tier,
            commands::pro_sync::pro_current_status,
            commands::pro_sync::pro_subscribe_sync_status,
            commands::pro_sync::pro_initiate_oauth,
            commands::pro_sync::pro_signout,
            commands::pro_sync::pro_enable_sync,
            commands::pro_sync::pro_activate_database,
            commands::pro_sync::pro_set_member,
            commands::pro_sync::pro_remove_member,
            commands::pro_sync::pro_leave_collection,
            commands::pro_sync::pro_list_members,
            commands::pro_sync::pro_create_invite,
            commands::pro_sync::pro_accept_invite,
            commands::pro_sync::pro_request_join,
            commands::pro_sync::pro_join_collection,
            commands::pro_sync::pro_list_joinable_collections,
            commands::pro_sync::pro_approve_request,
            commands::pro_sync::pro_list_invites,
            commands::pro_sync::pro_list_requests,
            commands::pro_sync::pro_revoke_invite,
            commands::pro_sync::pro_current_person,
            commands::recovered_items::pro_list_recovered_items,
            commands::recovered_items::pro_dismiss_recovered_item,
            commands::recovered_items::pro_clear_recovered_items,
            commands::embeddings::generate_root_embedding,
            commands::embeddings::search_roots,
            commands::embeddings::update_root_embedding,
            commands::embeddings::batch_generate_embeddings,
            commands::embeddings::on_root_closed,
            commands::embeddings::on_root_idle,
            commands::embeddings::sync_embeddings,
            commands::embeddings::get_stale_root_count,
            commands::nodes::create_node,
            commands::nodes::create_root_node,
            commands::nodes::create_node_mention,
            commands::nodes::get_node,
            commands::nodes::find_duplicate,
            commands::nodes::probe_and_recover_channel,
            commands::nodes::update_node,
            commands::nodes::move_node,
            commands::nodes::move_children_to_parent,
            commands::nodes::reorder_node,
            commands::nodes::delete_node,
            commands::nodes::get_children,
            commands::nodes::get_children_tree,
            commands::nodes::get_nodes_by_root_id,
            commands::nodes::query_nodes_simple,
            commands::nodes::mention_autocomplete,
            commands::nodes::save_node_with_parent,
            commands::nodes::get_outgoing_mentions,
            commands::nodes::get_incoming_mentions,
            commands::nodes::get_mentioning_roots,
            commands::nodes::get_node_relationships,
            commands::nodes::create_relationship,
            commands::nodes::delete_relationship,
            commands::nodes::update_relationship_properties,
            commands::nodes::delete_node_mention,
            commands::nodes::update_task_node,
            // Collection commands (browsing and management UI)
            commands::collections::get_all_collections,
            commands::collections::get_collection_members,
            commands::collections::get_collection_members_recursive,
            commands::collections::get_node_collections,
            commands::collections::add_node_to_collection,
            commands::collections::add_node_to_collection_path,
            commands::collections::remove_node_from_collection,
            commands::collections::find_collection_by_path,
            commands::collections::get_collection_by_name,
            commands::collections::create_collection,
            commands::collections::rename_collection,
            commands::collections::delete_collection,
            // Schema read commands (mutation commands removed, not used by UI)
            commands::schemas::get_all_schemas,
            commands::schemas::get_schema_definition,
            // File import commands for bulk markdown import
            commands::import::import_markdown_file,
            commands::import::import_markdown_files,
            commands::import::import_markdown_directory,
            // Settings commands
            commands::settings::get_settings,
            commands::settings::update_display_settings,
            commands::settings::get_capture_settings,
            commands::settings::update_capture_settings,
            commands::settings::get_openai_compat_configs,
            commands::settings::set_openai_compat_configs,
            // Local database registry commands (ADR-053)
            commands::database::get_daemon_version,
            commands::database::list_databases,
            commands::database::create_database,
            commands::database::register_database,
            commands::database::set_default_database,
            commands::database::rename_database,
            commands::database::remove_database,
            commands::database::set_active_database,
            commands::database::initial_database_id,
            // Local agent commands
            commands::local_agent::local_agent_status,
            commands::local_agent::local_agent_cancel_turn,
            commands::local_agent::ensure_model_ready,
            commands::local_agent::list_local_models,
            // Chat model management commands
            commands::chat_models::chat_model_list,
            commands::chat_models::chat_model_recommended,
            commands::chat_models::chat_model_download,
            commands::chat_models::chat_model_cancel_download,
            commands::chat_models::chat_model_delete,
            commands::chat_models::chat_model_load,
            commands::chat_models::chat_model_unload,
            commands::chat_models::get_system_ram_gb,
            // PTY agent session commands
            commands::agent_session::launch_session,
            commands::agent_session::write_input,
            commands::agent_session::resize_terminal,
            commands::agent_session::terminate_session,
            commands::agent_session::list_sessions,
            commands::agent_session::check_agent_availability,
            // First-launch onboarding wizard + Settings integrations panel
            commands::onboarding::check_onboarding_status,
            commands::onboarding::configure_path,
            commands::onboarding::remove_from_path,
            commands::onboarding::configure_skill,
            commands::onboarding::complete_onboarding,
            commands::onboarding::install_skill,
            commands::onboarding::remove_skill,
            commands::onboarding::get_skill_setup_status,
            commands::onboarding::get_integrations_status,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // Run with event handler for graceful shutdown. Routing lives in
    // `handle_run_event` (module-level, not inlined here) so it can be
    // exercised directly against `tauri::test`'s MockRuntime without a real
    // window — see `shutdown_tests`.
    app.run(move |app_handle, event| handle_run_event(app_handle, &event));
}

/// Route one `RunEvent` to the right shutdown action.
///
/// `WindowEvent::CloseRequested` deliberately does NOT call
/// [`graceful_shutdown`]. Tauri fires it the instant the OS asks to close a
/// window — before the frontend (`app-initialization.ts`'s
/// `onCloseRequested`) has had any chance to decide whether to veto it and
/// flush pending writes first. Whenever a frontend listener is registered
/// for that event, which ours always is, Tauri's own manager holds the
/// window open pending that decision regardless of what the listener
/// ultimately does (confirmed against the `tauri` 2.11 source —
/// `manager/window.rs`'s `on_window_event` calls `api.prevent_close()`
/// unconditionally whenever `has_js_listener` is true, before the frontend
/// callback has even run). The window only actually closes once the
/// frontend explicitly calls `currentWindow.destroy()` — immediately when
/// there's nothing to flush, or after `flushAllPending()` resolves (that
/// call self-bounds to 5s, so this never waits unboundedly). `destroy()`
/// bypasses `CloseRequested` entirely and is what drives `ExitRequested`
/// then `Exit`, so gating `graceful_shutdown` on those two instead means it
/// can only run once the flush has already finished or hit its own timeout —
/// never mid-flush, and never before a vetoed close's later retry.
///
/// A single-window assumption is baked into this today (one `ShutdownToken`
/// per app). Window-per-database work will need a per-window token instead —
/// not built here, see `ShutdownToken::shutdown_started`'s doc comment for
/// why the one-shot guard already lives somewhere that scales to that.
fn handle_run_event<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>, event: &tauri::RunEvent) {
    match event {
        tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { .. },
            ..
        } => {
            tracing::debug!(
                "Window '{}' close requested — deferring shutdown to ExitRequested/Exit so a \
                 frontend flush-before-close veto isn't cut short",
                label
            );
        }
        tauri::RunEvent::ExitRequested { code, .. } => {
            tracing::info!(
                "App exit requested (code: {:?}), performing graceful shutdown...",
                code
            );
            graceful_shutdown(app_handle);
        }
        tauri::RunEvent::Exit => {
            // Safety net in case ExitRequested was ever skipped — cancel()
            // and graceful_shutdown's guard are both idempotent, so this is
            // a no-op on the (expected) common path where ExitRequested
            // already ran it.
            tracing::info!("App exiting, ensuring shutdown signal sent...");
            graceful_shutdown(app_handle);
        }
        _ => {}
    }
}

/// Perform graceful shutdown: cancel background tasks.
///
/// Guarded by [`ShutdownToken::begin_shutdown_once`] so the sequence runs
/// exactly once per token even though `ExitRequested` and `Exit` can both
/// drive it for the same exit (see `handle_run_event`).
///
/// No blocking sleep here: the previous 200ms `std::thread::sleep` ran on
/// the Tauri event-loop thread — the same thread that services the webview —
/// which stalls the UI at exactly the moment a flush needs to make progress.
/// It also couldn't have helped the background tasks it was meant to give
/// time to: they run on the async runtime's own worker threads and react to
/// `cancel()` independently of how long this thread blocks afterward.
pub(crate) fn graceful_shutdown<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) {
    use tauri::Manager;

    let Some(shutdown_token) = app_handle.try_state::<ShutdownToken>() else {
        return;
    };

    if !shutdown_token.begin_shutdown_once() {
        tracing::debug!("Graceful shutdown already in progress, skipping duplicate call");
        return;
    }

    shutdown_token.cancel();
    tracing::info!("Shutdown: complete");
}

#[cfg(test)]
mod relaunch_tests {
    use super::parse_relaunch_database_arg;

    #[test]
    fn parses_database_id_from_argv() {
        let argv = vec![
            "/path/to/nodespace-app".to_string(),
            "--database".to_string(),
            "db-123".to_string(),
        ];
        assert_eq!(
            parse_relaunch_database_arg(&argv),
            Some("db-123".to_string())
        );
    }

    #[test]
    fn returns_none_without_the_flag() {
        let argv = vec!["/path/to/nodespace-app".to_string()];
        assert_eq!(parse_relaunch_database_arg(&argv), None);
    }

    #[test]
    fn returns_none_when_the_flag_has_no_value() {
        let argv = vec![
            "/path/to/nodespace-app".to_string(),
            "--database".to_string(),
        ];
        assert_eq!(parse_relaunch_database_arg(&argv), None);
    }

    #[test]
    fn returns_none_for_a_blank_value() {
        let argv = vec![
            "/path/to/nodespace-app".to_string(),
            "--database".to_string(),
            "   ".to_string(),
        ];
        assert_eq!(parse_relaunch_database_arg(&argv), None);
    }

    /// A flag value that happens to look like another flag is still just a
    /// value — parsing is positional (whatever follows `--database`), not a
    /// search for the next `--`-prefixed token.
    #[test]
    fn takes_the_very_next_token_even_if_flag_shaped() {
        let argv = vec![
            "/path/to/nodespace-app".to_string(),
            "--database".to_string(),
            "--not-actually-a-flag".to_string(),
        ];
        assert_eq!(
            parse_relaunch_database_arg(&argv),
            Some("--not-actually-a-flag".to_string())
        );
    }

    /// `handle_relaunch`'s early-return branch (no "main" window to focus)
    /// must not panic — this is the same shape of guard `graceful_shutdown`
    /// and `handle_run_event` already use `MockRuntime` to exercise. Doesn't
    /// touch `PENDING_TRAY_DATABASE_SELECTION` (the function returns before
    /// ever reaching it), so it's safe to run in parallel with the test below
    /// that does.
    #[test]
    fn handle_relaunch_without_a_main_window_does_not_panic() {
        let app = tauri::test::mock_app();
        super::handle_relaunch(
            &app.handle().clone(),
            &["--database".to_string(), "x".to_string()],
        );
    }

    /// Both halves of the pull-based-fallback contract live in one test:
    /// `PENDING_TRAY_DATABASE_SELECTION` is a process-wide `static`, so a
    /// separate "stashes nothing" test would race with a separate "stashes
    /// and is consumed" test under the default parallel test runner and
    /// flake (mirrors `resolve_ui_binary_honors_env_var` in the daemon
    /// crate's tray tests, which does the same for its own process-wide env
    /// var). Covers: a plain relaunch (no `--database` — the "Open
    /// NodeSpace" case) stashes nothing, so an unrelated later pull never
    /// sees a stale "open nothing in particular" mistaken for a real
    /// request; a relaunch naming a database stashes it (in addition to
    /// emitting the live event, not exercised here — that needs a real
    /// listener); a first pull observes it; a second pull observes nothing
    /// (consumed, not re-delivered).
    #[test]
    fn handle_relaunch_pull_based_fallback_stash_and_consume_cycle() {
        let app = tauri::test::mock_app();
        tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("failed to build mock window");
        let handle = app.handle().clone();

        super::handle_relaunch(&handle, &["/path/to/nodespace-app".to_string()]);
        assert_eq!(
            super::take_pending_tray_database_selection(),
            None,
            "a plain relaunch (no --database) must not stash anything"
        );

        super::handle_relaunch(
            &handle,
            &[
                "/path/to/nodespace-app".to_string(),
                "--database".to_string(),
                "db-456".to_string(),
            ],
        );
        assert_eq!(
            super::take_pending_tray_database_selection(),
            Some("db-456".to_string())
        );
        assert_eq!(
            super::take_pending_tray_database_selection(),
            None,
            "a second pull must not re-deliver an already-consumed selection"
        );
    }
}
