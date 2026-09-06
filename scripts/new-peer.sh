#!/usr/bin/env bash
# Generate a WireGuard peer keypair for Orion. Keys are created HERE, on the
# client machine — the server only ever receives the public key.
#
#   scripts/new-peer.sh <name> [fast|double|ghost] [ip]
#
# It prints (1) a snippet for ansible/host_vars/egress.yml and (2) the client
# profile to place at /etc/orion/profiles/<name>.conf once the server pubkey
# and endpoint are known (the playbook prints those after provisioning).
set -euo pipefail

NAME="${1:?usage: new-peer.sh <name> [fast|double|ghost] [ip]}"
MODE="${2:-fast}"
command -v wg >/dev/null 2>&1 || { echo "error: wireguard-tools required (pacman -S wireguard-tools)"; exit 1; }

case "$MODE" in
  fast)   BASE=10.66.0.10 ;;
  double) BASE=10.66.0.50 ;;
  ghost)  BASE=10.66.0.90 ;;
  *) echo "error: unknown mode '$MODE'"; exit 1 ;;
esac
IP="${3:-$BASE}"

umask 077
PRIV="$(wg genkey)"
PUB="$(printf '%s' "$PRIV" | wg pubkey)"

cat <<EOF

── 1. append to ansible/host_vars/egress-1.yml (under orion_peers:) ───────────
  - name: $NAME
    mode: $MODE
    ip: $IP
    pubkey: "$PUB"

then re-run:  ansible-playbook -i inventory.yml site.yml -K

── 2. client profile → /etc/orion/profiles/$NAME.conf ─────────────────────────
Fill in <SERVER_PUBLIC_KEY> and <ENDPOINT> from the playbook's report output.

[Interface]
PrivateKey = $PRIV
Address = $IP/32
DNS = 9.9.9.9
MTU = 1420

[Peer]
PublicKey = <SERVER_PUBLIC_KEY>
Endpoint = <ENDPOINT>
AllowedIPs = 0.0.0.0/0, ::/0
PersistentKeepalive = 25

────────────────────────────────────────────────────────────────────────────────
EOF
