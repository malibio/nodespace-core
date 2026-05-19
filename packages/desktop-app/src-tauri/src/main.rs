// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime");

    // Set this runtime as Tauri's async runtime before starting the app
    tauri::async_runtime::set(runtime.handle().clone());

    // Run the app within our custom runtime
    runtime.block_on(async { nodespace_app_lib::run() })
}
