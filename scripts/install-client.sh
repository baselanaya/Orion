#!/usr/bin/env bash
# Build + install the Orion client from source (Arch/CachyOS).
set -euo pipefail
CARGO="${CARGO:-$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/cargo}"
[ -x "$CARGO" ] || CARGO="$(command -v cargo)"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "==> building helper + app (release)"
(cd "$ROOT/client" && "$CARGO" build --release -p orion-helper -p orion-app)

echo "==> orion group + membership"
getent group orion >/dev/null || sudo groupadd -r orion
sudo usermod -aG orion "${USER:?}"

echo "==> installing helper + systemd unit"
sudo cp "$ROOT/client/target/release/orion-helper" /usr/local/bin/orion-helper
sudo chmod 755 /usr/local/bin/orion-helper
sudo cp "$ROOT/client/deploy/orion-helper.service" /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now orion-helper

echo
echo "done. app binary: $ROOT/client/target/release/orion-app"
echo "  - profiles live in /etc/orion/profiles/*.conf (root-owned 0600)"
echo "  - group membership needs a re-login before the app can reach the helper"
echo "  - a packaged .deb is also produced by: cd client/app/src-tauri && npx @tauri-apps/cli@2 build"
