//! Orion desktop UI shell. Unprivileged by design: every privileged action is
//! a request over the IPC socket to the root helper (orion-helper).

use orion_ipc::{Request, Response};

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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            status,
            list_profiles,
            connect,
            disconnect,
            new_identity
        ])
        .run(tauri::generate_context!())
        .expect("error while running orion");
}
