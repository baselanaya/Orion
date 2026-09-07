# Phase M — Mobile (Android)

Status: **live-verified 2026-09-07** on the emulator (tunnel up, internet exit
through the node, node-side transfer counters confirmed).

## What exists

- **Node listener**: plain WireGuard on `wg1` (`10.66.2.1/24`, UDP 51820) on the
  egress node. Mobile phones use standard WireGuard because AmneziaWG's Go
  userspace is not shipped for Android here; the obfuscated `awg0` listener stays
  for desktop. Traffic from the mobile pool (`wg1_pool`) is forwarded and
  masqueraded by the same consolidated nftables ruleset as the desktop pools.
- **App**: `client/android/` — Kotlin, single Activity, no Compose. Uses the
  official `com.wireguard.android:tunnel:1.0.20260102` artifact (GoBackend
  userspace, all 4 ABIs) with core-library desugaring. The manifest declares the
  library's own `GoBackend$VpnService`; the app only implements the `Tunnel`
  interface.
- **UI**: same navy system as the desktop (0E1A2B / 4C7DF0), ring + power button,
  HUD with rx/tx/handshake, DISCONNECT/CONNECT swap.

## Profile delivery (v1, no file picker yet)

The profile is passed into the app as a **base64 intent extra** (survives adb/am
shell quoting):

```bash
B64=$(base64 -w0 ~/.orion/mobile.conf)
adb shell am start -n dev.orion.mobile/.MainActivity \
  --es conf_b64 "$B64" --ez connect true
```

`--ez connect true` auto-connects right after import (used by the e2e; on a real
phone you'd tap the power button). The `conf` raw-text extra also works but newlines
must be escaped as literal `\n`.

## Build

```bash
cd client/android
echo "sdk.dir=$HOME/android-sdk" > local.properties
gradle assembleDebug          # debug-signed, sideload-ready
# APK: app/build/outputs/apk/debug/app-debug.apk (~19.9MB, 4 ABIs)
```

Gradle 8.9 + AGP 8.5.2 + Kotlin 2.0.0, JDK 17. Release builds are debug-signed on
purpose (sideload build; no store distribution).

## Emulator e2e (verified 2026-09-07)

1. Headless AVD `orion-test` (system-images;android-34;google_apis;x86_64).
2. `adb install -r app-debug.apk`; grant VPN without the dialog:
   `adb shell appops set dev.orion.mobile ACTIVATE_VPN allow`.
3. Import + auto-connect via the intent above.
4. Verified: `tun0` up with `10.66.2.11/32`; ping `10.66.2.1` (node wg1 gateway)
   0% loss ~77ms; ping `1.1.1.1` through the exit 0% loss ~86ms;
   `wg show wg1 transfer` on the node shows real encrypted bytes for the peer.

## Key handling

Mobile peer keys are X25519, generated **client-side** (`uv run` + cryptography);
the node only ever receives the public key (stored in `host_vars/egress-1.yml`,
gitignored; the active peer pubkey on the node must match). The client conf lives
at `~/.orion/mobile.conf` (0600) — never committed.

## Honest limits

- Mobile is **fast mode only** (no Tor/ghost from phones; Tor on mobile would need
  Orbot-style proxy chaining — future work).
- No kill switch on mobile yet: Android "Block connections without VPN" (lockdown
  mode) is the system-provided equivalent; the app doesn't toggle it
  programmatically yet (VpnService `ALWAYS_ON` + lockdown can be set via
  `VpnService.prepare`-adjacent APIs — roadmap).
- No file-picker import yet (intent extra + QR via `scripts/mobile-qr.sh` output
  is the path; QR decode in-app is roadmap).
