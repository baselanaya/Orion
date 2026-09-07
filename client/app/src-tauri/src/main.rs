//! Orion desktop client. Unprivileged by design: every privileged action is
//! a request over the IPC socket to the root helper (orion-helper).

use orion_ipc::{Request, Response};
use std::sync::OnceLock;
use tauri::{Emitter, Manager};

/// Set in setup; tray menu threads run outside the tauri closure scope.
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

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

#[tauri::command]
fn get_settings() -> Result<Response, String> {
    orion_ipc::call(&Request::GetSettings).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_settings(
    dns_mode: Option<String>,
    dns_custom: Option<String>,
    kill_switch: Option<String>,
    tor_browser_path: Option<String>,
) -> Result<Response, String> {
    orion_ipc::call(&Request::SetSettings {
        dns_mode,
        dns_custom,
        kill_switch,
        tor_browser_path,
    })
    .map_err(|e| e.to_string())
}

/// The UI's poll loop calls this with the live status; Rust rebuilds the tray
/// menu (per-server connect, state-aware items) and swaps the icon only when
/// something actually changed. `mode` is "fast"|"ghost"|None.
#[tauri::command]
fn tray_sync(
    app: tauri::AppHandle,
    state: String,
    profile: Option<String>,
    profiles: Vec<String>,
    mode: Option<String>,
) -> Result<(), String> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};

    let connected = state == orion_ipc::states::CONNECTED;
    let sig = format!("{state}|{profile:?}|{profiles:?}|{mode:?}");
    let last = app.state::<TraySig>();
    let mut guard = last.0.lock().map_err(|_| "tray state poisoned")?;
    if guard.as_deref() == Some(sig.as_str()) {
        return Ok(());
    }
    *guard = Some(sig);

    let tray = app
        .tray_by_id("orion-tray")
        .ok_or_else(|| "tray unavailable".to_string())?;

    // ---- icon + tooltip
    let icon = if connected && mode.as_deref() == Some("ghost") {
        tauri::include_image!("icons/tray-ghost.png")
    } else if connected {
        tauri::include_image!("icons/tray-on.png")
    } else {
        tauri::include_image!("icons/tray-off.png")
    };
    let _ = tray.set_icon(Some(icon));
    let _ = tray.set_tooltip(Some(match (&state, mode.as_deref()) {
        (s, _) if s == orion_ipc::states::LOCKED_NO_TUNNEL => "Orion — LOCKED (no tunnel)".to_string(),
        (_, Some("ghost")) => format!("Orion — SECURED (ghost) via {}", profile.clone().unwrap_or_default()),
        (s, _) if s == orion_ipc::states::CONNECTED => format!("Orion — SECURED via {}", profile.clone().unwrap_or_default()),
        _ => "Orion — offline".to_string(),
    }));

    // ---- menu
    let header_text = if connected {
        format!("secured — {}", profile.clone().unwrap_or_else(|| "?".into()))
    } else if state == orion_ipc::states::LOCKED_NO_TUNNEL {
        "locked: kill switch active, no tunnel".to_string()
    } else {
        "offline".to_string()
    };
    let header = MenuItemBuilder::with_id("header", header_text)
        .enabled(false)
        .build(&app)
        .map_err(|e| e.to_string())?;

    let mut mb = MenuBuilder::new(&app).item(&header);

    if !connected {
        let sub = SubmenuBuilder::new(&app, "Connect").build().map_err(|e| e.to_string())?;
        let mut any = false;
        for p in &profiles {
            if Some(p) == profile.as_ref() {
                continue;
            }
            let item = MenuItemBuilder::with_id(format!("profile:{p}"), p)
                .build(&app)
                .map_err(|e| e.to_string())?;
            sub.append(&item).map_err(|e| e.to_string())?;
            any = true;
        }
        if any {
            mb = mb.item(&sub);
        }
    } else {
        let disc = MenuItemBuilder::with_id("disconnect", "Disconnect").build(&app).map_err(|e| e.to_string())?;
        mb = mb.item(&disc);
    }

    let newid = MenuItemBuilder::with_id("newid", "New identity (Ghost)").build(&app).map_err(|e| e.to_string())?;
    let tb = MenuItemBuilder::with_id("torbrowser", "Launch Tor Browser").build(&app).map_err(|e| e.to_string())?;
    let show = MenuItemBuilder::with_id("show", "Show Orion").build(&app).map_err(|e| e.to_string())?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Orion").build(&app).map_err(|e| e.to_string())?;
    let menu = mb
        .items(&[&newid, &tb, &show, &quit])
        .build()
        .map_err(|e| e.to_string())?;
    tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    Ok(())
}

struct TraySig(std::sync::Mutex<Option<String>>);

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(TraySig(std::sync::Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            status,
            list_profiles,
            connect,
            disconnect,
            new_identity,
            launch_tor_browser,
            get_settings,
            set_settings,
            tray_sync
        ])
        .setup(|app| {
            let _ = APP_HANDLE.set(app.handle().clone());
            // system tray: closed window keeps Orion running in the background.
            // The tray-icon crate panics (it does not return Err) when no
            // appindicator library is present — e.g. on CI runners — so the
            // whole setup is catch_unwind-guarded and degrades gracefully.
            let tray_setup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> tauri::Result<()> {
                use tauri::menu::{MenuBuilder, MenuItemBuilder};
                use tauri::tray::TrayIconBuilder;

                let header = MenuItemBuilder::with_id("header", "offline")
                    .enabled(false)
                    .build(app)?;
                let show = MenuItemBuilder::with_id("show", "Show Orion").build(app)?;
                let newid = MenuItemBuilder::with_id("newid", "New identity (Ghost)").build(app)?;
                let tb = MenuItemBuilder::with_id("torbrowser", "Launch Tor Browser").build(app)?;
                let quit = MenuItemBuilder::with_id("quit", "Quit Orion").build(app)?;
                let menu = MenuBuilder::new(app).items(&[&header, &show, &newid, &tb, &quit]).build()?;

                TrayIconBuilder::with_id("orion-tray")
                    .icon(tauri::include_image!("icons/tray-off.png"))
                    .tooltip("Orion — offline")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| {
                        let id = event.id().as_ref().to_string();
                        match id.as_str() {
                            "show" => {
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                            s if s.starts_with("profile:") => {
                                let name = s.trim_start_matches("profile:").to_string();
                                std::thread::spawn(move || {
                                    match orion_ipc::call(&Request::Connect { profile: name.clone() }) {
                                        Ok(Response::Err { message }) => {
                                            if let Some(app) = APP_HANDLE.get() {
                                                let _ = app.emit("tray-error", message);
                                            }
                                        }
                                        _ => {}
                                    }
                                });
                            }
                            "disconnect" => {
                                std::thread::spawn(|| {
                                    let _ = orion_ipc::call(&Request::Disconnect);
                                });
                            }
                            "torbrowser" => {
                                std::thread::spawn(|| {
                                    match orion_ipc::call(&Request::LaunchTorBrowser) {
                                        Ok(Response::Err { message }) => {
                                            if let Some(app) = APP_HANDLE.get() {
                                                let _ = app.emit("tray-error", message);
                                            }
                                        }
                                        Ok(_) => {
                                            if let Some(app) = APP_HANDLE.get() {
                                                let _ = app.emit("tray-tb", true);
                                            }
                                        }
                                        Err(_) => {
                                            if let Some(app) = APP_HANDLE.get() {
                                                let _ = app.emit("tray-tb", false);
                                            }
                                        }
                                    }
                                });
                            }
                            "newid" => {
                                let _ = app.emit("tray-new-id", ());
                            }
                            "quit" => app.exit(0),
                            _ => {}
                        }
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
