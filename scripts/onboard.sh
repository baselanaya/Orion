#!/usr/bin/env bash
# Orion onboarding — rent any Debian 12/13 VPS, enter its IP, get a working VPN.
#
#   scripts/onboard.sh
#
# Asks for your node's SSH details, provisions it (hardening, AmneziaWG,
# Tor, kill-switch firewall), generates a client profile, and leaves the
# desktop app ready to connect. Keys are generated on this machine; the node
# only ever receives public keys.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "Orion onboarding"
echo "================"
echo "Rent a Debian 12/13 VPS from any provider, then answer:"
echo
read -rp "Node IP or hostname: " NODE
read -rp "SSH user [root]: " SUSER; SUSER=${SUSER:-root}
read -rp "SSH port [22]: " SPORT; SPORT=${SPORT:-22}
read -rp "Node name [egress-1]: " NNAME; NNAME=${NNAME:-egress-1}
read -rp "Profile name for this device [laptop]: " PEER; PEER=${PEER:-laptop}

command -v uv >/dev/null 2>&1 || { echo "error: install uv first (https://docs.astral.sh/uv/)"; exit 1; }
uv sync --quiet

# 1. ssh key for the node
[ -f "$HOME/.ssh/orion_ed25519" ] || ssh-keygen -t ed25519 -f "$HOME/.ssh/orion_ed25519" -N "" -C "orion-$NNAME"
echo
echo "-> if the provider panel is still open, paste this key into its SSH-key"
echo "   field; otherwise enter the node password at the prompt:"
cat "$HOME/.ssh/orion_ed25519.pub"
echo
ssh-copy-id -i "$HOME/.ssh/orion_ed25519.pub" -p "$SPORT" "$SUSER@$NODE"

# 2. inventory (local + gitignored)
mkdir -p ansible/host_vars
cat > ansible/inventory.yml <<EOF
all:
  children:
    orion_nodes:
      children:
        egress:
    egress:
      hosts:
        $NNAME:
          ansible_host: $NODE
          ansible_port: $SPORT
          ansible_user: $SUSER
          ansible_ssh_private_key_file: $HOME/.ssh/orion_ed25519
          orion_ssh_port: $SPORT
          orion_wg_port: 4500
EOF

# 3. client keypair (generated here; the node only receives the public key)
KEYS="$(uv run --with cryptography python - <<'PY'
import base64
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey
k = X25519PrivateKey.generate()
priv = base64.b64encode(k.private_bytes_raw()).decode()
pub = base64.b64encode(k.public_key().public_bytes_raw()).decode()
print(priv)
print(pub)
PY
)"
PRIV="$(head -1 <<< "$KEYS")"
PUB="$(tail -1 <<< "$KEYS")"
[ -n "$PRIV" ] && [ -n "$PUB" ] || { echo "error: key generation failed"; exit 1; }

# 4. host_vars
cat > "ansible/host_vars/$NNAME.yml" <<EOF
orion_peers:
  - name: $PEER
    mode: fast
    ip: 10.66.0.10
    pubkey: "$PUB"
EOF

# 5. provision
echo
echo "==> provisioning $NNAME (hardening, AmneziaWG, Tor, kill-switch firewall)"
uv run ansible-playbook -i ansible/inventory.yml ansible/site.yml

# 6. server public key
SPUB="$(ssh -p "$SPORT" "$SUSER@$NODE" 'wg pubkey < /etc/wireguard/egress.key')"
[ -n "$SPUB" ] || { echo "error: could not read the server public key"; exit 1; }

# 7. client profile
sudo mkdir -p /etc/orion/profiles
sudo tee "/etc/orion/profiles/$PEER.conf" > /dev/null <<EOF
[Interface]
PrivateKey = $PRIV
Address = 10.66.0.10/32
DNS = 9.9.9.9
MTU = 1420

[Peer]
PublicKey = $SPUB
Endpoint = $NODE:4500
AllowedIPs = 0.0.0.0/0, ::/0
PersistentKeepalive = 25
EOF
sudo chmod 600 "/etc/orion/profiles/$PEER.conf"

echo
echo "==> done."
echo "    client profile : /etc/orion/profiles/$PEER.conf"
echo "    launch the Orion app and press connect"
