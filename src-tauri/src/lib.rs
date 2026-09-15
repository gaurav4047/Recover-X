//! RecoverX Tauri application library.
//!
//! Exposes Tauri commands for the React frontend.
//! All heavy I/O happens in async Tauri commands backed by Tokio.

pub mod commands;
pub mod state;

use state::AppState;
use tauri::Manager;
use tauri::Emitter;

/// Entry point called by main.rs.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    recoverx_logging::init(recoverx_logging::LogConfig {
        level: std::env::var("RECOVERX_LOG").unwrap_or_else(|_| "info".to_string()),
        json: false,
        log_file: None,
    })
    .expect("Failed to initialise logging");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // Device commands
            commands::devices::list_devices,
            commands::devices::get_device_info,
            // Session commands
            commands::sessions::create_session,
            commands::sessions::list_sessions,
            commands::sessions::load_session,
            commands::sessions::delete_session,
            // Scan commands
            commands::scan::start_scan,
            commands::scan::pause_scan,
            commands::scan::resume_scan,
            commands::scan::cancel_scan,
            // Results and recovery commands
            commands::results::list_recovered_files,
            commands::results::query_recovered_files,
            commands::results::get_category_counts,
            commands::results::list_partitions,
            commands::results::recover_files,
            commands::results::verify_file_hash,
            // Application info
            commands::app::get_app_info,
            commands::app::get_common_locations,
        ])
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");

            let state = app.state::<AppState>();
            state
                .initialise(data_dir)
                .expect("Failed to initialise app state");

            // Forward RecoverXEvents from the orchestrator's EventBus to the
            // Tauri window event system so the React frontend can listen.
            if let Ok(orchestrator) = state.orchestrator() {
                let mut rx = orchestrator.event_bus.subscribe();
                let app_handle = app.handle().clone();

                // Use Tauri's runtime — tokio::spawn is not available in setup()
                tauri::async_runtime::spawn(async move {
                    loop {
                        match rx.recv().await {
                            Ok(event) => {
                                if let Ok(payload) = serde_json::to_string(&event) {
                                    let _ = app_handle.emit("recoverx-event", payload);
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                continue;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                break;
                            }
                        }
                    }
                });
            }

            tracing::info!("RecoverX started");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Error while running RecoverX");
}
