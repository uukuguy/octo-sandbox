mod commands;
pub mod server;
mod tray;

use tauri::Manager;

pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_port,
            commands::get_version,
            commands::get_app_info,
            commands::get_dashboard_url,
        ])
        .setup(|app| {
            tray::create_tray(app)?;

            // Start the embedded dashboard server on a background tokio task.
            // The Tauri runtime provides a tokio runtime, so we can spawn directly.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match server::EmbeddedServer::start().await {
                    Ok(srv) => {
                        tracing::info!(port = srv.port, "Embedded server ready");
                        // Navigate the main WebView to the embedded server URL
                        if let Some(window) = handle.get_webview_window("main") {
                            let url = format!("http://127.0.0.1:{}", srv.port);
                            let _ = window.navigate(url.parse().unwrap());
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to start embedded server");
                    }
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            // Close-to-tray: hide the window instead of quitting when user clicks X.
            // The user can fully quit via the tray menu "Quit" option.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}
