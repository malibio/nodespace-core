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

// gRPC-backed node event watcher (#1114). Inert until activated by #1113 —
// see watcher.rs module docs for activation gating.
pub mod watcher;

// launchd daemon lifecycle (Issue #1179) — macOS only
#[cfg(target_os = "macos")]
pub mod daemon_setup;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn toggle_sidebar() -> String {
    "Sidebar toggled!".to_string()
}

/// Report the current daemon health to the frontend.
///
/// Returns "healthy", "starting", or "not_running". The frontend uses this
/// to decide whether to show an error state (Issue #1179).
#[tauri::command]
async fn check_daemon_status() -> String {
    #[cfg(target_os = "macos")]
    {
        use daemon_setup::{check_daemon_socket, DaemonStatus};

        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return "not_running".to_string(),
        };
        let socket_path = home.join(crate::constants::DAEMON_SOCKET_RELATIVE);
        return match check_daemon_socket(socket_path.as_path()).await {
            DaemonStatus::Healthy => "healthy".to_string(),
            DaemonStatus::Starting => "starting".to_string(),
            DaemonStatus::NotRunning => "not_running".to_string(),
        };
    }
    #[cfg(not(target_os = "macos"))]
    "healthy".to_string()
}

// Include test module
#[cfg(test)]
mod tests;

/// Shared shutdown token for graceful background task termination.
///
/// Managed as Tauri state so it can be accessed from both the setup phase
/// (where background tasks are spawned) and the run event handler (where
/// shutdown is triggered). When cancelled, all background tasks exit their
/// loops before the Tokio runtime drops.
#[derive(Clone)]
pub struct ShutdownToken(tokio_util::sync::CancellationToken);

impl ShutdownToken {
    fn new() -> Self {
        Self(tokio_util::sync::CancellationToken::new())
    }

    /// Create a child token for a background task.
    /// Cancelling the parent automatically cancels all children.
    pub fn child_token(&self) -> tokio_util::sync::CancellationToken {
        self.0.child_token()
    }

    /// Signal all background tasks to shut down.
    /// Idempotent - safe to call multiple times.
    pub fn cancel(&self) {
        self.0.cancel();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::{menu::*, Emitter, Manager, RunEvent};

    // Initialize tracing — respects RUST_LOG env var, defaults to info for nodespace_app
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("nodespace_app=info")),
        )
        .try_init()
        .ok();

    // Create shutdown token for coordinating graceful background task termination
    let shutdown_token = ShutdownToken::new();
    let shutdown_token_for_setup = shutdown_token.clone();

    let app = tauri::Builder::default()
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

            let new_database = MenuItemBuilder::new("New Database...")
                .id("new_database")
                .build(app)?;

            let open_database = MenuItemBuilder::new("Open Database...")
                .id("open_database")
                .build(app)?;

            let open_settings = MenuItemBuilder::new("Settings...")
                .id("open_settings")
                .accelerator("CmdOrCtrl+,")
                .build(app)?;

            let db_separator = PredefinedMenuItem::separator(app)?;
            let settings_separator = PredefinedMenuItem::separator(app)?;

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
                    &new_database,
                    &open_database,
                    &db_separator,
                    &import_submenu,
                    &settings_separator,
                    &open_settings,
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

            // Spawn async task to start daemon (if needed) then connect gRPC client.
            // setup() is synchronous so we can't block_on here — spawn a task instead.
            // manage(GrpcClient) happens inside the task; commands that need it will
            // fail gracefully until the connection is established.
            #[cfg(unix)]
            {
                use tauri::Emitter;

                let app_handle = app.handle().clone();
                let session_token = shutdown_token_for_setup.child_token();

                tauri::async_runtime::spawn(async move {
                    // macOS: ensure nodespaced launchd agent is installed and running.
                    #[cfg(target_os = "macos")]
                    {
                        use daemon_setup::{ensure_daemon_running, DaemonStatus};
                        match ensure_daemon_running(&app_handle).await {
                            Ok(DaemonStatus::Healthy) => {
                                tracing::info!("nodespaced is running");
                            }
                            Ok(status) => {
                                tracing::warn!("nodespaced not yet healthy: {:?}", status);
                            }
                            Err(e) => {
                                tracing::error!("Daemon setup failed: {:#}", e);
                            }
                        }
                    }

                    // Connect gRPC client over UDS.
                    match crate::services::GrpcClient::connect().await {
                        Ok(grpc_client) => {
                            app_handle.manage(grpc_client);
                            tracing::info!("gRPC client connected to nodespaced");
                            watcher::spawn(app_handle.clone(), session_token);
                        }
                        Err(e) => {
                            tracing::error!("Failed to connect to nodespaced: {e:#}");
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.emit("daemon-status", "not_running");
                            }
                        }
                    }
                });
            }

            // Streaming task registry for PTY session cancellation (Issue #1120)
            app.manage(commands::agent_session::StreamingTaskRegistry::default());

            Ok(())
        })
        .on_menu_event(|app, event| {
            let toggle_sidebar_id = MenuId::new("toggle_sidebar");
            let toggle_status_bar_id = MenuId::new("toggle_status_bar");
            let quit_id = MenuId::new("quit");
            let import_folder_id = MenuId::new("import_folder");
            let new_database_id = MenuId::new("new_database");
            let open_database_id = MenuId::new("open_database");
            let open_settings_id = MenuId::new("open_settings");

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
            } else if *event.id() == new_database_id || *event.id() == open_database_id {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("menu-select-database", ());
                }
            } else if *event.id() == open_settings_id {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("menu-open-settings", ());
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
            check_daemon_status,
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
            commands::nodes::update_node,
            commands::nodes::move_node,
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
            commands::nodes::delete_node_mention,
            commands::nodes::update_task_node,
            // Collection commands (Issue #757 - Collection browsing and management UI)
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
            // Schema read commands (Issue #690 - mutation commands removed, not used by UI)
            commands::schemas::get_all_schemas,
            commands::schemas::get_schema_definition,
            // File import commands for bulk markdown import
            commands::import::import_markdown_file,
            commands::import::import_markdown_files,
            commands::import::import_markdown_directory,
            // Settings commands
            commands::settings::get_settings,
            commands::settings::update_display_settings,
            commands::settings::select_new_database,
            commands::settings::restart_app,
            commands::settings::reset_database_to_default,
            commands::settings::get_capture_settings,
            commands::settings::update_capture_settings,
            // Local agent commands (Issue #1008)
            commands::local_agent::local_agent_status,
            commands::local_agent::local_agent_new_session,
            commands::local_agent::local_agent_send,
            commands::local_agent::local_agent_cancel,
            commands::local_agent::local_agent_end_session,
            commands::local_agent::local_agent_get_sessions,
            commands::local_agent::ensure_model_ready,
            commands::local_agent::list_local_models,
            // Chat model management commands (Issue #1008)
            commands::chat_models::chat_model_list,
            commands::chat_models::chat_model_recommended,
            commands::chat_models::chat_model_download,
            commands::chat_models::chat_model_cancel_download,
            commands::chat_models::chat_model_delete,
            commands::chat_models::chat_model_load,
            commands::chat_models::chat_model_unload,
            commands::chat_models::ollama_available,
            commands::chat_models::get_system_ram_gb,
            // PTY agent session commands (Issue #1120)
            commands::agent_session::launch_session,
            commands::agent_session::write_input,
            commands::agent_session::resize_terminal,
            commands::agent_session::terminate_session,
            commands::agent_session::list_sessions,
            commands::agent_session::check_agent_availability,
            // First-launch onboarding wizard (Issue #1180)
            commands::onboarding::check_onboarding_status,
            commands::onboarding::configure_path,
            commands::onboarding::configure_mcp,
            commands::onboarding::configure_skill,
            commands::onboarding::complete_onboarding,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // Run with event handler for graceful shutdown
    let shutdown_token_for_events = shutdown_token.clone();
    app.run(move |app_handle, event| match event {
        RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { .. },
            ..
        } => {
            tracing::info!(
                "Window '{}' close requested, performing graceful shutdown...",
                label
            );
            graceful_shutdown(app_handle);
        }
        RunEvent::ExitRequested { code, .. } => {
            tracing::info!(
                "App exit requested (code: {:?}), performing graceful shutdown...",
                code
            );
            graceful_shutdown(app_handle);
        }
        RunEvent::Exit => {
            tracing::info!("App exiting, ensuring shutdown signal sent...");
            shutdown_token_for_events.cancel();
        }
        _ => {}
    });
}

/// Perform graceful shutdown: cancel background tasks and exit cleanly.
///
/// Guarded by an `AtomicBool` because Tauri may fire both `CloseRequested` and
/// `ExitRequested` events, and we must only run the shutdown sequence once.
pub(crate) fn graceful_shutdown(app_handle: &tauri::AppHandle) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tauri::Manager;

    static SHUTDOWN_ONCE: AtomicBool = AtomicBool::new(false);
    if SHUTDOWN_ONCE.swap(true, Ordering::SeqCst) {
        tracing::debug!("Graceful shutdown already in progress, skipping duplicate call");
        return;
    }

    if let Some(shutdown_token) = app_handle.try_state::<ShutdownToken>() {
        shutdown_token.cancel();
    }
    // Grace period for background tasks (watcher) to exit their tokio::select!
    // loops and drop their Arc references before the runtime drops.
    std::thread::sleep(std::time::Duration::from_millis(200));

    tracing::info!("Shutdown: complete");
}
