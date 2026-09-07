//! Wire protocol between the Orion desktop UI and the privileged root helper.
//!
//! Requests and responses are single-line JSON over a Unix socket
//! (`/run/orion/helper.sock`, group `orion`). Kept dependency-light and
//! inspectable on purpose — this boundary is the whole security model:
//! the UI is unprivileged, the helper owns the kernel state.

use std::io::{BufRead, BufReader, Write};

use serde::{Deserialize, Serialize};

pub const DEFAULT_SOCKET_PATH: &str = "/run/orion/helper.sock";

/// Tunnel states reported by the helper.
/// `locked_no_tunnel` is the fail-closed degraded state: the kill-switch
/// rules are live but the WireGuard interface is gone — nothing egresses.
pub mod states {
    pub const DISCONNECTED: &str = "disconnected";
    pub const CONNECTED: &str = "connected";
    pub const LOCKED_NO_TUNNEL: &str = "locked_no_tunnel";
}

/// A connectable profile plus its tunnel address. The address is what lets
/// the UI derive the peer's mode: the IPv4 must fall in one of the node's
/// mode pools (plan §5.3: .10-.49 fast, .50-.89 double, .90-.129 ghost).
#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub name: String,
    pub addr: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Status,
    ListProfiles,
    Connect { profile: String },
    Disconnect,
    /// Signal NEWNYM on the node's Tor control port (Ghost "new identity").
    NewIdentity,
    /// Locate and launch a Tor Browser installation for the desktop user.
    LaunchTorBrowser,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Status {
        state: String,
        profile: Option<String>,
        endpoint: Option<String>,
        handshake_age_secs: Option<u64>,
        rx_bytes: Option<u64>,
        tx_bytes: Option<u64>,
        detail: Option<String>,
    },
    Profiles {
        profiles: Vec<ProfileInfo>,
    },
    Err {
        message: String,
    },
}

impl Response {
    pub fn disconnected() -> Self {
        Response::Status {
            state: states::DISCONNECTED.into(),
            profile: None,
            endpoint: None,
            handshake_age_secs: None,
            rx_bytes: None,
            tx_bytes: None,
            detail: None,
        }
    }
}

#[derive(Debug)]
pub enum IpcError {
    Io(std::io::Error),
    Json(serde_json::Error),
    ServerClosed,
    /// The IPC transport does not exist on this platform
    /// (the root helper itself is Linux-only today).
    Unsupported(&'static str),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::Io(e) => write!(f, "helper socket: {e} (is orion-helper running?)"),
            IpcError::Json(e) => write!(f, "protocol error: {e}"),
            IpcError::ServerClosed => write!(f, "helper closed the connection"),
            IpcError::Unsupported(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for IpcError {}

#[cfg(unix)]
pub fn call(req: &Request) -> Result<Response, IpcError> {
    unix_transport::call_at(DEFAULT_SOCKET_PATH, req)
}

#[cfg(unix)]
pub use unix_transport::call_at;

#[cfg(unix)]
mod unix_transport {
    use super::*;
    use std::os::unix::net::UnixStream;

    pub fn call_at(path: &str, req: &Request) -> Result<Response, IpcError> {
        let mut stream = UnixStream::connect(path).map_err(IpcError::Io)?;
        let mut line = serde_json::to_string(req).map_err(IpcError::Json)?;
        line.push('\n');
        stream.write_all(line.as_bytes()).map_err(IpcError::Io)?;
        stream.flush().map_err(IpcError::Io)?;

        let mut reader = BufReader::new(stream);
        let mut buf = String::new();
        let n = reader.read_line(&mut buf).map_err(IpcError::Io)?;
        if n == 0 {
            return Err(IpcError::ServerClosed);
        }
        serde_json::from_str(buf.trim()).map_err(IpcError::Json)
    }
}

#[cfg(not(unix))]
pub fn call(_req: &Request) -> Result<Response, IpcError> {
    Err(IpcError::Unsupported(
        "helper IPC is Linux-only in this build",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip() {
        let req = Request::Connect {
            profile: "laptop".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"connect\""));
        let back: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Request::Connect { profile } if profile == "laptop"));
    }

    #[test]
    fn response_roundtrip() {
        let res = Response::disconnected();
        let json = serde_json::to_string(&res).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Response::Status { ref state, .. } if state == states::DISCONNECTED));
    }
}
