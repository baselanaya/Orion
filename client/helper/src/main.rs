//! orion-helper — the privileged half of the Orion client.
//!
//! Runs as root (systemd unit in client/deploy/). The desktop UI talks to it
//! over /run/orion/helper.sock; everything that touches the kernel — the
//! nftables kill switch and wg-quick — happens here and only here.
//!
//! Fail-closed semantics (plan v1.2 §5.7):
//!   - On connect, the kill-switch ruleset is installed BEFORE the tunnel
//!     comes up: default-drop, with explicit allows only (loopback, tunnel
//!     interface, the resolved WireGuard endpoint, DHCP renewals).
//!   - Rules live in the kernel, not in this process — a helper crash leaves
//!     the machine locked, not exposed. Status reports `locked_no_tunnel`.
//!   - Rules are removed only on an *intentional* disconnect (or the manual
//!     `orion-helper unlock` emergency command).
//!   - If the tunnel itself fails to come up, the lock is rolled back — a
//!     failed connection request must not blackhole the machine silently.

use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, ToSocketAddrs};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use orion_ipc::states::{CONNECTED, DISCONNECTED, LOCKED_NO_TUNNEL};
use orion_ipc::{ProfileInfo, Request, Response};

const IFACE: &str = "orion0";
const RUN_DIR: &str = "/run/orion";
const SOCKET_PATH: &str = "/run/orion/helper.sock";
const PROFILES_DIR: &str = "/etc/orion/profiles";
const WG_CONF: &str = "/etc/wireguard/orion0.conf";
const AWG_CONF: &str = "/etc/amnezia/amneziawg/orion0.conf";
const ACTIVE_FILE: &str = "/run/orion/active-profile";
const ACTIVE_TOOL: &str = "/run/orion/active-tool";
const KS_TABLE: &str = "inet orion_ks";

/// Which userspace tool owns the current tunnel: "wg" (kernel wireguard) or
/// "awg" (kernel amneziawg). Decided per-profile: junk params (Jc) mean AWG.
fn active_tool() -> &'static str {
    match std::fs::read_to_string(ACTIVE_TOOL) {
        Ok(t) if t.starts_with("awg") => "awg-quick",
        _ => "wg-quick",
    }
}

fn show_binary() -> &'static str {
    if active_tool() == "awg-quick" { "awg" } else { "wg" }
}

/// Serializes all mutating operations; status shares it for consistency.
static APP_LOCK: Mutex<()> = Mutex::new(());

fn main() {
    match std::env::args().nth(1).unwrap_or_default().as_str() {
        "serve" => serve(),
        "status" => {
            let _guard = APP_LOCK.lock().unwrap();
            println!("{}", serde_json::to_string_pretty(&status()).unwrap());
        }
        "unlock" => {
            let _guard = APP_LOCK.lock().unwrap();
            unlock();
        }
        _other => {
            eprintln!("usage: orion-helper [serve|status|unlock]");
            std::process::exit(2);
        }
    }
}

fn serve() {
    let _ = std::fs::create_dir_all(RUN_DIR);
    let _ = std::fs::remove_file(SOCKET_PATH); // stale socket from a crash
    let listener =
        UnixListener::bind(SOCKET_PATH).expect("cannot bind helper socket — run as root");

    // group `orion` may talk to us; the UI runs unprivileged
    std::fs::set_permissions(&SOCKET_PATH, std::fs::Permissions::from_mode(0o660)).ok();
    let _ = Command::new("chgrp").args(["orion", SOCKET_PATH]).status();

    eprintln!("[orion-helper] listening on {SOCKET_PATH}");
    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            thread::spawn(|| handle_conn(stream));
        }
    }
}

fn handle_conn(stream: std::os::unix::net::UnixStream) {
    // Access is gated by the socket itself: /run/orion is root:orion 0750 and
    // the socket is 0660 root:orion, so only members of the `orion` group
    // (i.e. the desktop user we blessed) can connect at all.
    let reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut writer = stream;
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => dispatch(&req),
            Err(e) => Response::Err {
                message: format!("bad request: {e}"),
            },
        };
        let mut out = match serde_json::to_string(&response) {
            Ok(s) => s,
            Err(_) => break,
        };
        out.push('\n');
        if writer.write_all(out.as_bytes()).is_err() {
            break;
        }
        let _ = writer.flush();
    }
}

fn dispatch(req: &Request) -> Response {
    let _guard = APP_LOCK.lock().unwrap();
    match req {
        Request::Status => status(),
        Request::ListProfiles => list_profiles(),
        Request::Connect { profile } => {
            eprintln!("[orion-helper] connect profile={profile}");
            let r = connect_req(profile);
            eprintln!("[orion-helper] connect -> {r:?}");
            r
        }
        Request::Disconnect => {
            eprintln!("[orion-helper] disconnect requested");
            let r = disconnect_req();
            eprintln!("[orion-helper] disconnect -> {r:?}");
            r
        }
        Request::NewIdentity => {
            eprintln!("[orion-helper] new identity (NEWNYM)");
            let r = new_identity_req();
            eprintln!("[orion-helper] new identity -> {r:?}");
            r
        }
    }
}

// ---------------------------------------------------------------- profiles

struct Profile {
    name: String,
    path: PathBuf,
    endpoint: Option<String>,
    addr: Option<String>,
    awg: bool,
    _dns: Vec<String>,
}

fn load_profile(name: &str) -> Result<Profile, String> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("invalid profile name".into());
    }
    let path = Path::new(PROFILES_DIR).join(format!("{name}.conf"));
    let text = fs_read(&path)?;
    let mut section = String::new();
    let mut endpoint = None;
    let mut addr = None;
    let mut awg = false;
    let mut dns = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            section = line.to_ascii_lowercase();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            match (section.as_str(), key.as_str()) {
                ("[peer]", "endpoint") => endpoint = Some(val.to_string()),
                ("[interface]", "address") => addr = Some(val.to_string()),
                ("[interface]", "jc") | ("[interface]", "h1") => awg = true,
                ("[interface]", "dns") => dns.extend(
                    val.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                ),
                _ => {}
            }
        }
    }
    Ok(Profile {
        name: name.to_string(),
        path,
        endpoint,
        addr,
        awg,
        _dns: dns,
    })
}

fn fs_read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|_| format!("cannot read {}", path.display()))
}

fn list_profiles() -> Response {
    let mut profiles: Vec<ProfileInfo> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(PROFILES_DIR) {
        for e in entries.flatten() {
            let fname = e.file_name().to_string_lossy().to_string();
            if let Some(stem) = fname.strip_suffix(".conf") {
                let addr = load_profile(stem).ok().and_then(|p| p.addr);
                profiles.push(ProfileInfo {
                    name: stem.to_string(),
                    addr,
                });
            }
        }
    }
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Response::Profiles { profiles }
}

// ------------------------------------------------------------- kill switch

struct Resolved {
    v4: Option<String>,
    v6: Option<String>,
    port: u16,
}

fn resolve_endpoint(spec: &str) -> Result<Resolved, String> {
    let (host, port_s) = if let Some(rest) = spec.strip_prefix('[') {
        // [2001:db8::1]:51820
        let close = rest.find(']').ok_or("malformed IPv6 endpoint")?;
        let host = &rest[..close];
        let port = rest[close + 1..].trim_start_matches(':');
        (host.to_string(), port.to_string())
    } else {
        let idx = spec.rfind(':').ok_or("endpoint must be host:port")?;
        (spec[..idx].to_string(), spec[idx + 1..].to_string())
    };
    let port: u16 = port_s.parse().map_err(|_| "bad endpoint port")?;

    // Resolve BEFORE the lock goes up — name resolution itself needs the
    // network we are about to block.
    let addrs = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {host}: {e}"))?;
    let mut r = Resolved {
        v4: None,
        v6: None,
        port,
    };
    for a in addrs {
        match a.ip() {
            IpAddr::V4(v) => {
                if r.v4.is_none() {
                    r.v4 = Some(v.to_string());
                }
            }
            IpAddr::V6(v) => {
                if r.v6.is_none() {
                    r.v6 = Some(v.to_string());
                }
            }
        }
        if r.v4.is_some() && r.v6.is_some() {
            break;
        }
    }
    if r.v4.is_none() && r.v6.is_none() {
        return Err(format!("no addresses for {host}"));
    }
    Ok(r)
}

fn apply_lock(ep: &Resolved) -> Result<(), String> {
    let mut script = format!(
        "table {KS_TABLE}\n\
         delete table {KS_TABLE}\n\
         table {KS_TABLE} {{\n\
         \x20 chain output {{\n\
         \x20   type filter hook output priority filter; policy drop;\n\
         \x20   oifname \"lo\" accept\n\
         \x20   oifname \"{IFACE}\" accept\n"
    );
    if let Some(v4) = &ep.v4 {
        script += &format!("    meta nfproto ipv4 ip daddr {v4} udp dport {} accept\n", ep.port);
    }
    if let Some(v6) = &ep.v6 {
        script += &format!("    meta nfproto ipv6 ip6 daddr {v6} udp dport {} accept\n", ep.port);
    }
    script += "    udp sport 68 udp dport 67 accept\n"; // DHCP renewals
    script += "    udp sport 546 udp dport 547 accept\n"; // DHCPv6
    script += "  }\n}\n";
    run("nft", &["-f", "-"], Some(&script)).map(|_| ())
}

fn remove_lock() {
    if run("nft", &["list", "table", KS_TABLE], None).is_ok() {
        let _ = run("nft", &["delete", "table", KS_TABLE], None);
    }
}

// ----------------------------------------------------------------- process

fn run(cmd: &str, args: &[&str], input: Option<&str>) -> Result<String, String> {
    let mut c = Command::new(cmd);
    c.args(args);
    if input.is_some() {
        c.stdin(Stdio::piped());
    }
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| format!("{cmd}: {e}"))?;
    if let Some(text) = input {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| format!("{cmd}: {e}"))?;
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(format!(
            "{cmd} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

// ------------------------------------------------------------- tunnel mgmt

fn iface_exists() -> bool {
    Path::new("/sys/class/net").join(IFACE).exists()
}

/// wg-quick needs resolvconf or systemd-resolved to apply `DNS =` lines;
/// degrade gracefully (strip them) when neither exists.
fn have_dns_hook() -> bool {
    if Path::new("/usr/bin/resolvconf").exists() || Path::new("/usr/sbin/resolvconf").exists() {
        return true;
    }
    Command::new("systemctl")
        .args(["is-active", "--quiet", "systemd-resolved"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn build_wg_conf(profile: &Profile) -> Result<String, String> {
    let text = fs_read(&profile.path)?;
    let has_dns_hook = have_dns_hook();
    let mut out = String::new();
    let mut in_interface = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_interface = line.eq_ignore_ascii_case("[interface]");
        }
        let key = line.to_ascii_lowercase();
        // DNS lines need a resolvconf hook; without one they are stripped
        if in_interface && key.starts_with("dns") && !has_dns_hook {
            continue;
        }
        // hardening: wg-quick/awg-quick would execute these as root — our
        // profiles never ship them (defense in depth against a poisoned
        // profile file)
        if key.starts_with("postup")
            || key.starts_with("preup")
            || key.starts_with("postdown")
            || key.starts_with("predown")
            || key.starts_with("table")
        {
            continue;
        }
        out.push_str(raw);
        out.push('\n');
    }
    Ok(out)
}

fn connect_req(name: &str) -> Response {
    match connect_inner(name) {
        Ok(r) => r,
        Err(e) => Response::Err { message: e },
    }
}

fn connect_inner(name: &str) -> Result<Response, String> {
    if name.trim().is_empty() {
        return Err("no profile selected".into());
    }
    if iface_exists() {
        bring_down_tunnel();
    }

    let profile = load_profile(name)?;
    let spec = profile
        .endpoint
        .clone()
        .ok_or("profile has no Endpoint line")?;
    let ep = resolve_endpoint(&spec)?;

    // 1. lock first (fail-closed), 2. then tunnel.
    apply_lock(&ep)?;

    // AWG profiles (junk params present) go through awg-quick, plain
    // WireGuard profiles through wg-quick; interface name is orion0 either way.
    let (quick, conf_path) = if profile.awg {
        ("awg-quick", AWG_CONF)
    } else {
        ("wg-quick", WG_CONF)
    };

    let conf = build_wg_conf(&profile)?;
    if let Some(parent) = Path::new(conf_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(conf_path, conf).map_err(|e| format!("write {conf_path}: {e}"))?;
    std::fs::set_permissions(conf_path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;

    if let Err(e) = run("timeout", &["25", quick, "up", IFACE], None) {
        // Fresh failure is intentional — roll the lock back so the machine
        // isn't left dark; only crashes/disconnects keep the lock.
        let _ = run("timeout", &["10", quick, "down", IFACE], None);
        remove_lock();
        return Err(format!("{quick} up failed: {e}"));
    }

    let _ = std::fs::write(ACTIVE_FILE, &profile.name);
    let _ = std::fs::write(ACTIVE_TOOL, if profile.awg { "awg" } else { "wg" });
    Ok(status())
}

fn bring_down_tunnel() {
    if iface_exists() {
        let quick = active_tool();
        let _ = run(&quick, &["down", IFACE], None);
    }
    let _ = std::fs::remove_file(ACTIVE_FILE);
    let _ = std::fs::remove_file(ACTIVE_TOOL);
}

fn disconnect_req() -> Response {
    let mut warnings = Vec::new();
    if iface_exists() {
        let quick = active_tool();
        if let Err(e) = run("timeout", &["10", &quick, "down", IFACE], None) {
            warnings.push(format!("{quick} down: {e}"));
        }
    }
    let _ = std::fs::remove_file(ACTIVE_FILE);
    // intentional disconnect — the lock comes down with it
    remove_lock();
    if warnings.is_empty() {
        Response::Ok
    } else {
        Response::Err {
            message: warnings.join("; "),
        }
    }
}

/// SIGNAL NEWNYM over the node's Tor control port (reachable only through
/// the tunnel at 10.66.0.1:9051). The passphrase lives at
/// /etc/orion/control_pass; the node pins the matching hash in torrc.
fn new_identity_req() -> Response {
    use std::io::{BufRead, BufReader};
    if !iface_exists() {
        return Response::Err {
            message: "tunnel is down; connect before requesting a new identity".into(),
        };
    }
    let pass = match std::fs::read_to_string("/etc/orion/control_pass") {
        Ok(p) => p.trim().to_string(),
        Err(_) => {
            return Response::Err {
                message: "no control password on file (/etc/orion/control_pass)".into(),
            }
        }
    };
    let stream = match std::net::TcpStream::connect("10.66.0.1:9051") {
        Ok(s) => s,
        Err(e) => return Response::Err { message: format!("control port unreachable: {e}") },
    };
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => return Response::Err { message: format!("{e}") },
    };
    let mut reader = BufReader::new(reader_stream);
    let mut writer = stream;
    for cmd in [format!("AUTHENTICATE \"{pass}\""), "SIGNAL NEWNYM".into(), "QUIT".into()] {
        if writer
            .write_all(format!("{cmd}\r\n").as_bytes())
            .is_err()
        {
            return Response::Err { message: "control connection dropped".into() };
        }
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => return Response::Err { message: "control connection closed".into() },
            Ok(_) => {
                if !line.starts_with("250") {
                    return Response::Err { message: format!("tor rejected: {}", line.trim()) };
                }
            }
            Err(e) => return Response::Err { message: format!("control read: {e}") },
        }
    }
    Response::Ok
}

fn unlock() {
    bring_down_tunnel();
    remove_lock();
    println!("orion unlocked: tunnel down, kill switch removed");
}

// ------------------------------------------------------------------ status

fn wg_field(field: &str) -> Option<String> {
    let bin = show_binary();
    run(bin, &["show", IFACE, field], None)
        .ok()
        .and_then(|out| out.lines().next().map(|l| l.to_string()))
}

fn status() -> Response {
    let profile = std::fs::read_to_string(ACTIVE_FILE)
        .ok()
        .map(|s| s.trim().to_string());
    let iface_up = iface_exists();
    let locked = run("nft", &["list", "table", KS_TABLE], None).is_ok();

    let mut endpoint = None;
    let mut handshake_age_secs = None;
    let mut rx_bytes = None;
    let mut tx_bytes = None;

    if iface_up {
        if let Some(line) = wg_field("endpoints") {
            if let Some((_, ep)) = line.split_once('\t') {
                endpoint = Some(ep.trim().to_string());
            }
        }
        if let Some(line) = wg_field("latest-handshakes") {
            if let Some((_, t)) = line.split_once('\t') {
                if let Ok(t) = t.trim().parse::<u64>() {
                    if t > 0 {
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        handshake_age_secs = Some(now.saturating_sub(t));
                    }
                }
            }
        }
        if let Some(line) = wg_field("transfer") {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() == 3 {
                rx_bytes = parts[1].trim().parse().ok();
                tx_bytes = parts[2].trim().parse().ok();
            }
        }
    }

    let state = if iface_up {
        CONNECTED
    } else if locked {
        // helper died after locking, or the tunnel dropped underneath us:
        // fail-closed degraded state, nothing egresses.
        LOCKED_NO_TUNNEL
    } else {
        DISCONNECTED
    };

    Response::Status {
        state: state.to_string(),
        profile,
        endpoint,
        handshake_age_secs,
        rx_bytes,
        tx_bytes,
        detail: None,
    }
}
