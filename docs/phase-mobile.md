# Phase M — Mobile (Android)

Status: **live-verified 2026-09-07** on the emulator — tunnel up, internet exit
through the node, **QR import, profile persistence, and always-on/lockdown
auto-reconnect across a full reboot** all green.

## What exists

- **Node listener**: plain WireGuard on `wg1` (`10.66.2.1/24`, UDP 51820) on the
  egress node. Mobile phones use standard WireGuard because AmneziaWG's Go
  userspace is not shipped for Android here; the obfuscated `awg0` listener stays
  for desktop. Traffic from the mobile pool (`wg1_pool`) is forwarded and
  masqueraded by the same consolidated nftables ruleset as the desktop pools.
- **App**: `client/android/` — Kotlin, single Activity, no Compose. Uses the
  official `com.wireguard.android:tunnel:1.0.20260102` artifact (GoBackend
  userspace, all 4 ABIs) with core-library desugaring + ZXing core for QR.
  The manifest declares the library's own `GoBackend$VpnService`; the app only
  implements the `Tunnel` interface (as a process-wide singleton — GoBackend
  tracks the active tunnel by object identity).
- **Profile import**: in-app **IMPORT QR** button (document picker → ZXing
  decode → config parse) or the base64 intent extra (`conf_b64`, + optional
  `--ez connect true`). Imported profiles persist in app-private storage and
  reload at launch.
- **Always-on / kill switch**: profiles persist in app-private storage;
  `OrionApp` registers `GoBackend.setAlwaysOnCallback`, so when Android starts
  the service in always-on mode (boot, network change) the tunnel reconnects
  itself with boot-time retries. The in-app "Always-on and kill switch ->" row
  deep-links to the system VPN page where the user arms Always-on + "Block
  connections without VPN" (lockdown). Android has no public API to arm those
  toggles programmatically — the deep link is the honest path.
- **UI**: same navy system as the desktop (0E1A2B / 4C7DF0), ring + power button,
  HUD with rx/tx/handshake, DISCONNECT/CONNECT swap.

## Profile delivery

Phone flow: generate the QR on the desktop (`scripts/mobile-qr.sh`), import via
the app's IMPORT QR button. Development/automation flow:

```bash
B64=$(base64 -w0 ~/.orion/mobile.conf)
adb shell am start -n dev.orion.mobile/.MainActivity \
  --es conf_b64 "$B64" --ez connect true
```

The `conf` raw-text extra also works but newlines must be escaped as literal `\n`
(base64 avoids adb shell quoting mangling entirely).

## Build

```bash
cd client/android
echo "sdk.dir=$HOME/android-sdk" > local.properties
gradle assembleDebug          # debug-signed, sideload-ready
# APK: app/build/outputs/apk/debug/app-debug.apk (~20MB, 4 ABIs)
```

Gradle 8.9 + AGP 8.5.2 + Kotlin 2.0.0, JDK 17. Release builds are debug-signed on
purpose (sideload build; no store distribution).

## Emulator e2e (verified 2026-09-07)

1. Headless AVD `orion-test` (system-images;android-34;google_apis;x86_64).
2. `adb install -r app-debug.apk`; grant VPN without the dialog:
   `adb shell appops set dev.orion.mobile ACTIVATE_VPN allow`.
3. **QR import**: QR pushed to `/sdcard/Download/`, IMPORT QR → document picker →
   decode → profile line + persist (`run-as dev.orion.mobile ls files/` shows
   `orion.conf`). CONNECT → `tun0` up `10.66.2.11/32`, exit ping ~77ms.
4. **Always-on + reboot**: `settings put secure always_on_vpn_app
   dev.orion.mobile` + `always_on_vpn_lockdown 1`, then `adb reboot`. After boot,
   **without opening the app**: `tun0` up, exit ping ~77ms, fresh handshake in
   `wg show wg1 latest-handshakes` on the node. Opening the app shows SECURED
   with live stats (state sharing via the singleton Tunnel works).
5. Intent-extra import path re-verified (`conf_b64` + auto-connect).

### Always-on gotchas (cost us a debugging cycle)

- The service declaration **must** carry the `android.net.VpnService`
  intent-filter — the system's always-on resolver finds the service by action,
  and silently resets `always_on_vpn_app` at validation time without it.
- `settings put secure always_on_vpn_app` is the dev/test path for arming
  always-on; a real phone does it in Settings (the app deep-links there).

## Key handling

Mobile peer keys are X25519, generated **client-side** (`uv run` + cryptography);
the node only ever receives the public key (stored in `host_vars/egress-1.yml`,
gitignored; the active peer pubkey on the node must match). The client conf lives
at `~/.orion/mobile.conf` (0600) — never committed. On the device the profile
sits in app-private storage (`files/orion.conf`), reachable only by the app uid.

## Honest limits

- Mobile is **fast mode only** (no Tor/ghost from phones; Tor on mobile would need
  Orbot-style proxy chaining — future work).
- The kill switch is the **system lockdown toggle** (armed by the user via the
  deep link); the app cannot arm it programmatically with public APIs.
- No camera scan (image-file QR only) — camera scanning wants
  zxing-android-embedded + a live camera, next round if wanted.
- iOS has no client.
