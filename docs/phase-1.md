# Phase 1 — Foundation (runbook)

Roadmap step 1 of plan v1.2: hardened node provisioning, the egress node's Fast
path (single-hop WireGuard + NAT + QUIC/IPv6 drop), and the Linux client
skeleton — unprivileged Tauri UI + root helper with the fail-closed nftables
kill switch.

```
ansible/               provisioning — site.yml, roles/common, roles/wireguard_egress
client/protocol/       orion-ipc   — line-JSON wire types over a Unix socket
client/helper/         orion-helper — root daemon: owns nftables + wg-quick
client/deploy/         orion-helper.service (systemd unit)
client/app/            Tauri shell — src-tauri (Rust) + ui/ (static, no bundler)
scripts/new-peer.sh    peer keypair generator (keys never leave the client)
```

## 1. Provision the node

```sh
uv sync                      # controller side (once): provisions .venv with ansible-core
cp ansible/inventory.example.yml ansible/inventory.yml   # edit host/user/ports
cp ansible/host_vars/egress-1.example.yml ansible/host_vars/egress-1.yml
uv run ansible-playbook -i ansible/inventory.yml ansible/site.yml -K
```

The playbook: key-only SSH (password auth off, fail2ban, optional port
migration with 22 kept open until confirmed), journald forced volatile (no
persistent logs), unattended security upgrades, and one consolidated nftables
ruleset — input locked to SSH + WireGuard, forward locked to the fast pool
with **UDP/443 (QUIC) and all IPv6 dropped**, masquerade out the WAN
interface. It finishes by printing the server's **public key** and endpoint
for client profiles.

Port migration: keep `orion_ssh_keep_port_22: true` on first run; after
confirming the new port works, set it `false` and update `ansible_port`.

## 2. Create a peer

```sh
scripts/new-peer.sh laptop fast
# → paste the pubkey snippet into ansible/host_vars/egress-1.yml, re-run playbook
# → keep the printed private key for the client profile
```

Client profile on the **client machine** (root-owned, mode 0600):

```
/etc/orion/profiles/laptop.conf
```

using the `[Interface]`/`[Peer]` template from the script's output, the server
public key and endpoint from step 1. `AllowedIPs = 0.0.0.0/0, ::/0` and
`DNS = 9.9.9.9` (used only if resolvconf/systemd-resolved exists; otherwise
the helper strips the line and DNS is DoH-inside-tunnel as per plan §5.6).

## 3. Install the client (Linux)

Dependencies: `wireguard-tools` (provides wg + wg-quick), nftables, rust.
Note: on this machine plain `cargo` is a broken rustup shim — use
`~/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/cargo`.

```sh
cd client
cargo build --release -p orion-helper -p orion-app

sudo groupadd -r orion && sudo usermod -aG orion $USER   # re-login after
sudo cp target/release/orion-helper /usr/local/bin/
sudo cp deploy/orion-helper.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now orion-helper

target/release/orion-app
```

What connect does, in order: resolve the profile's endpoint (network is still
open), install the kill-switch table (`inet orion_ks`: default-drop, allow
loopback / tunnel interface / endpoint UDP / DHCP renewals), then `wg-quick
up orion0`. The rules live in the kernel — if the helper or app dies, the
machine stays locked (`locked_no_tunnel` in the UI), never exposed. Rules are
removed only by an intentional disconnect, or:

```sh
sudo orion-helper unlock        # emergency manual teardown
```

A failed connect (bad profile, unreachable endpoint) rolls the lock back so a
mistake can't silently blackhole the machine.

## Validation status

Verified statically on 2026-09-06:
- `cargo check` clean workspace-wide on Rust edition 2024 (orion-ipc,
  orion-helper, orion-app on Tauri 2.11 — the latest stable line); protocol
  wire-format unit tests pass.
- `ansible-playbook --syntax-check` passes (ansible-core 2.21 via uv).
- `uv run python scripts/validate_ansible.py`: all playbook YAML parses, all
  templates render under strict-undefined.
- `nft -c` parse-clean for both the node firewall (rendered) and the helper's
  kill-switch ruleset shape.

Not yet exercised live: real VPS run, handshake, DNS behavior, suspend/resume.
`wireguard-tools` is not on this dev machine yet — install before the client
run (`sudo pacman -S wireguard-tools`).

## Hands off until later phases

- `double_pool` / `ghost_pool` nft sets exist but have no forward rules —
  chaining (roadmap 4) and Tor (roadmap 2) will add them, fail-closed by default.
- AmneziaWG (roadmap 3) replaces the plain-WireGuard handshake parameters.
