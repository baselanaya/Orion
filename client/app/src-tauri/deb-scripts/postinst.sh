#!/bin/bash
set -e

# Group + desktop user (first non-system user) so the unprivileged UI can
# reach the helper's IPC socket.
getent group orion >/dev/null || groupadd -r orion
U="$(id -un 1000 2>/dev/null || true)"
if [ -n "$U" ]; then
  usermod -aG orion "$U" 2>/dev/null || true
fi

chmod 755 /usr/local/bin/orion-helper 2>/dev/null || true
chmod 644 /etc/systemd/system/orion-helper.service 2>/dev/null || true

if command -v systemctl >/dev/null 2>&1; then
  systemctl daemon-reload || true
  systemctl enable --now orion-helper || true
fi

echo "orion: helper service enabled."
echo "  - profiles live in /etc/orion/profiles/*.conf (root-owned, 0600)"
echo "  - desktop users need the 'orion' group (re-login) to reach the helper"
exit 0
