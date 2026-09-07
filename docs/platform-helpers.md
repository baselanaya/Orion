# Native tunnel helpers — Windows & macOS design spec

Roadmap item: per-OS native helpers so Orion's device-wide, fail-closed
tunnel works beyond Linux. This document is the design of record; it exists
so the work can be executed (and reviewed) incrementally without re-deriving
the architecture.

## Status

| Platform | Tunnel | Kill switch | Helper IPC | State |
|---|---|---|---|---|
| Linux | `awg-quick` (kernel AmneziaWG) | nftables (fail-closed table `orion_ks`) | Unix socket `/run/orion/helper.sock` | **shipped** |
| Windows | WireGuardNT driver (`wireguard-nt` crate) or wireguard-go | Windows Filtering Platform (WFP) filters | Named pipe `\\.\pipe\orion-helper` | designed, not built |
| macOS | wireguard-go (user-space) or Network Extension | pf anchor rules | Unix socket in app container | designed, not built |

## Shared contract

Everything above the platform line stays identical: the helper owns the
tunnel lifecycle + kill switch, the UI is unprivileged, and the fail-closed
semantics hold everywhere:

1. Rules install **before** the tunnel interface comes up.
2. Default-drop; explicit allows only (loopback, tunnel interface, resolved
   endpoint, DHCP).
3. Rules persist in the OS firewall across helper crashes (`locked_no_tunnel`
   semantics on Windows/macOS too).
4. Rules are removed only on intentional disconnect or the documented
   emergency unlock.
5. A failed bring-up rolls the lock back — a mistake never silently
   blackholes the machine.

## Windows design

- **Tunnel**: WireGuardNT — the in-box-quality kernel driver shipped by the
  WireGuard project (`wireguard-nt` Rust bindings), falling back to
  wireguard-go. AmneziaWG obfuscation on Windows requires the AWG fork of
  the driver: vendor the amneziawg-windows driver build into our installer
  (signed) — this is the main risk item.
- **Kill switch**: WFP (Windows Filtering Platform) session rules —
  `windivert` is *not* sufficient; use the
  [`wfp-bindmap`/`windows` crate WFP APIs] or a tiny companion service.
  Sublayers: permit loopback + tunnel interface + resolved endpoint;
  block everything else, IPv4+IPv6, at the ALE layers.
- **Helper process**: a Windows service (LocalSystem) named
  `orion-helper`, IPC over a named pipe with the same line-JSON protocol;
  UI authenticates the pipe owner.
- **Configuration**: profiles live under `C:\ProgramData\Orion\profiles`.
- **Effort**: ~2-4 weeks for a hardened v1 (WFP is the long pole);
  feature scopes identical to Linux.

## macOS design

- **Tunnel**: wireguard-go user-space (like WireGuard's mac app) or the
  AWG fork; a Network Extension is the App-Store-grade path but requires
  Apple signing — v1 uses a launchd `root` daemon + wireguard-go.
- **Kill switch**: pf anchor (`orion`) loaded at tunnel-up
  (`pfctl -a orion -f rules`), default-drop with pass rules for the tunnel
  interface and endpoint; rules persist across helper crashes and are
  removed on intentional disconnect/unlock.
- **Helper process**: launchd daemon (root), Unix socket in
  `/var/run/orion/helper.sock`, same protocol.
- **Signing/notarization**: required for the .app to run on stock macOS —
  an Apple Developer account is a hard dependency for distribution.
- **Effort**: ~1-2 weeks for v1 (pf is well-trodden; the signing pipeline
  is the friction).

## Non-goals (explicit)

- iOS/Android: different architecture entirely (VPN profiles + NEPacketTunnel
  / VpnService) — not planned.
- Full GUI settings parity on Windows/macOS at v1: the UI ships, the
  platform-specific feature parity lands with the native helpers above.
