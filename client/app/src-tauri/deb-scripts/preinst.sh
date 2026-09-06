#!/bin/bash
# Pre-install: stop the helper so its binary can be replaced safely.
if command -v systemctl >/dev/null 2>&1; then
  systemctl stop orion-helper 2>/dev/null || true
fi
exit 0
