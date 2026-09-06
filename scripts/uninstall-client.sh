#!/usr/bin/env bash
# Remove the Orion client from this machine. Your node is untouched; client
# profiles in /etc/orion/profiles are kept unless you pass --purge.
set -euo pipefail
if command -v systemctl >/dev/null 2>&1; then
  sudo systemctl disable --now orion-helper 2>/dev/null || true
  sudo rm -f /etc/systemd/system/orion-helper.service
  sudo systemctl daemon-reload
fi
sudo rm -f /usr/local/bin/orion-helper
echo "orion client removed."
if [ "${1:-}" = "--purge" ]; then
  sudo rm -rf /etc/orion
  echo "profiles removed (--purge)."
else
  echo "profiles kept in /etc/orion/profiles (pass --purge to remove)."
fi
