//! Orion desktop client. Unprivileged by design: every privileged action is
//! a request over the IPC socket to the root helper (orion-helper).

use orion_ipc::{Request, Response};
use tauri::{Emitter, Manager};

#[tauri::command]
fn status() -> Result<Response, String> {
    orion_ipc::call(&Request::Status).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_profiles() -> Result<Response, String> {
    orion_ipc::call(&Request::ListProfiles).map_err(|e| e.to_string())
}

#[tauri::command]
fn connect(profile: String) -> Result<Response, String> {
    orion_ipc::call(&Request::Connect { profile }).map_err(|e| e.to_string())
}

#[tauri::command]
fn disconnect() -> Result<Response, String> {
    orion_ipc::call(&Request::Disconnect).map_err(|e| e.to_string())
}

#[tauri::command]
fn new_identity() -> Result<Response, String> {
    orion_ipc::call(&Request::NewIdentity).map_err(|e| e.to_string())
}

#[tauri::command]
fn launch_tor_browser() -> Result<Response, String> {
    orion_ipc::call(&Request::LaunchTorBrowser).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            status,
            list_profiles,
            connect,
            disconnect,
            new_identity,
            launch_tor_browser
        ])
        .setup(|app| {
            // system tray: closed window keeps Orion running in the background.
            // The tray-icon crate panics (it does not return Err) when no
            // appindicator library is present — e.g. on CI runners — so the
            // whole setup is catch_unwind-guarded and degrades gracefully.
            let tray_setup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> tauri::Result<()> {
                use tauri::menu::{MenuBuilder, MenuItemBuilder};
                use tauri::tray::TrayIconBuilder;

                let show = MenuItemBuilder::with_id("show", "Show Orion").build(app)?;
                let toggle = MenuItemBuilder::with_id("toggle", "Connect / Disconnect").build(app)?;
                let newid = MenuItemBuilder::with_id("newid", "New identity (Ghost)").build(app)?;
                let sep = tauri::menu::PredefinedMenuItem::separator(app)?;
                let quit = MenuItemBuilder::with_id("quit", "Quit Orion").build(app)?;
                let menu = MenuBuilder::new(app).items(&[&show, &toggle, &newid, &sep, &quit]).build()?;

                TrayIconBuilder::with_id("orion-tray")
                    .icon(tauri::include_image!("icons/32x32.png"))
                    .tooltip("Orion")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id().as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "toggle" => {
                            let _ = app.emit("tray-toggle", ());
                        }
                        "newid" => {
                            let _ = app.emit("tray-new-id", ());
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .build(app).map(|_| ())
            }));
            match tray_setup {
                Err(panic_payload) => {
                    eprintln!("[orion] system tray setup panicked; continuing without it");
                    let _ = panic_payload;
                }
                Ok(Err(e)) => eprintln!("[orion] tray setup error: {e}"),
                Ok(Ok(())) => {}
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // closing the window hides it to the tray instead of quitting
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running orion");
}
