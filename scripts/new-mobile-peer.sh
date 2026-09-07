#!/usr/bin/env bash
# Orion onboarding for MOBILE devices (Android/iOS WireGuard clients).
# Generates a standard-WireGuard keypair for a phone and prints:
#   1. the peer snippet to paste into ansible/host_vars/<node>.yml
#   2. the QR code to scan with the WireGuard mobile app (after provisioning)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NODE="${1:-egress-1}"
PEER="${2:-mobile}"
WGPORT="${3:-51820}"

command -v uv >/dev/null 2>&1 || { echo "error: uv required"; exit 1; }
KEYS="$(uv run --with cryptography python - <<'PY'
import base64
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey
k = X25519PrivateKey.generate()
print(base64.b64encode(k.private_bytes_raw()).decode())
print(base64.b64encode(k.public_key().public_bytes_raw()).decode())
PY
)"
PRIV="$(head -1 <<< "$KEYS")"
PUB="$(tail -1 <<< "$KEYS")"
IP="$(uv run --with cryptography python - <<PY
import sys
# first free address in the mobile pool 10.66.2.10-49
import glob, yaml
used = set()
for f in glob.glob('$ROOT/ansible/host_vars/*.yml'):
    try:
        d = yaml.safe_load(open(f)) or {}
    except Exception:
        continue
    for peer in (d.get('orion_wg1_peers') or []):
        used.add(peer.get('ip'))
for n in range(10, 50):
    ip = f'10.66.2.{n}'
    if ip not in used:
        print(ip); break
PY
)"

cat <<EOF

Peer:      $PEER ($PEER@mobile)
Address:   $IP/32  (mobile pool 10.66.2.0/24)

1) paste into ansible/host_vars/$NODE.yml:

orion_wg1_peers:
  - name: $PEER
    ip: $IP
    pubkey: "$PUB"

   then: scripts/node-update.sh

2) mobile profile (scan the QR printed by scripts/mobile-qr.sh, or import
   the .conf file into the WireGuard/AmneziaWG mobile app):

[Interface]
PrivateKey = $PRIV
Address = $IP/32
DNS = 9.9.9.9
MTU = 1420

[Peer]
PublicKey = <server wg1 public key: see playbook report>
Endpoint = <node-ip>:$WGPORT
AllowedIPs = 0.0.0.0/0, ::/0
PersistentKeepalive = 25
EOF
