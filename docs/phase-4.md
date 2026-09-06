# Phase 4 — entry-node chaining (runbook)

Roadmap step 4: Double mode. The client connects to the **entry** node, which
policy-routes per mode: Fast clients NAT directly out the entry's WAN; Double
clients ride the plain-WireGuard **hop** to the egress (sources preserved);
Ghost clients ride the hop and get force-redirected into Tor at the egress.

```
client ──AWG(4500)──▶ ENTRY ──┬─ fast:   NAT out entry WAN
  (10.66.1.x)                 └─ WG-hop(4501) ──▶ EGRESS ──┬─ double: NAT out egress
   src preserved in hop                                     └─ ghost: Tor
```

Status: roles built, egress hop deployed and live. The entry droplet itself is
the remaining piece.

## What exists

- `ansible/roles/entry_node` — client-facing AWG (awg0, 10.66.1.1/24, port
  4500), the hop (`wg-hop`, 10.66.255.1/30 → egress 10.66.255.2, port 4501,
  plain WG, `Table = off` + fwmark policy routing), and the entry nftables
  ruleset (mangle-marks fast→4500 / double+ghost→4501; `ip rule` sends marked
  traffic into the hop; fast NATs directly).
- Egress additions: `wg-hop` (10.66.255.2/30, peer = entry, AllowedIPs includes
  10.66.1.0/24 so transit sources validate), transit-pool nft rules
  (hop_fast/hop_double NAT at the egress; hop_ghost force-redirected into Tor),
  and Tor bound on both 10.66.0.1 and 10.66.255.2 (TransPort/DNSPort).
- Hop keys + entry-client keys are staged in `host_vars/entry-1.yml` and
  `host_vars/egress-1.yml` (gitignored).

## Bringing up the entry node

1. Create the droplet (same recipe as the egress): DigitalOcean → Droplet →
   Debian 13 → Basic Regular $4 → **region closest to you** (this is the node
   your latency depends on) → paste the orion SSH key → hostname `orion-entry-1`.
2. Send the IP; it gets added to `ansible/inventory.yml` under `entry`.
3. `uv run ansible-playbook -i ansible/inventory.yml ansible/site.yml` —
   hardening + AWG + hop + rules all deploy.
4. The playbook prints the entry's client-facing **public key**; client
   profiles for the entry paths are then written to `/etc/orion/profiles/`:
   - `laptop-entry.conf` — Address 10.66.1.10/32, Endpoint `<entry-ip>:4500`
     (Fast via entry),
   - `ghost-entry.conf` — Address 10.66.1.90/32, same endpoint (Ghost).
   Both carry the same AWG parameters as the node (Jc/Jmin/Jmax/S1/S2/H1-H4).

## Verification matrix (once profiles exist)

| Check | laptop-entry (fast) | ghost-entry (ghost) |
|---|---|---|
| exit IP | entry's WAN IP | Tor exit |
| DNS | via tunnel | captured by Tor DNSPort |
| QUIC / ICMP to internet | QUIC dropped, ICMP ok | dropped |
| hop integrity | n/a | src 10.66.1.90 visible at egress (`wg show wg-hop` transfer grows) |

Latency note (plan §9 step 4): measure RTT through the chain vs direct; the
double-hop stays only if it earns its RTT.

## Design notes

- The hop is plain WireGuard: that leg carries already-obfuscated client
  traffic between our own nodes, and its transport never traverses a
  client-visible path.
- Sources are preserved through the hop so every egress-side enforcement rule
  (QUIC drop, Tor force-redirect, per-mode firewalling) keeps working by
  source pool — the entry never NATs double/ghost traffic.
- The entry's hop socket pins fwmark 4502 so its own transport can't loop into
  a client tunnel; client traffic is marked 4500 (fast, → WAN) or 4501
  (double/ghost, → hop) by the mangle chain.
