#!/usr/bin/env bash
# Generate a scannable QR code from an Orion tunnel profile (.conf).
# Scan it with the WireGuard / AmneziaWG app on the phone and the profile
# imports instantly. The QR contains the full profile INCLUDING the private
# key — keep the rendered file safe and delete it after import.
#
#   scripts/mobile-qr.sh <profile.conf>
set -euo pipefail
CONF="${1:?usage: mobile-qr.sh <profile.conf>}"
[ -r "$CONF" ] || { echo "error: cannot read $CONF"; exit 1; }
OUT="${CONF%.conf}.qr.png"
uv run --with "qrcode[pil]" python - "$CONF" "$OUT" <<'PY'
import sys
import qrcode
conf = open(sys.argv[1]).read().strip()
img = qrcode.make(conf, border=2)
img.save(sys.argv[2])
print("QR saved:", sys.argv[2])
PY
echo "scan it with the WireGuard app on the phone, then DELETE this png:"
echo "  rm $OUT"
