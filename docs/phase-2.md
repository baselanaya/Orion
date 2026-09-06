# Phase 2 — Tor core (runbook)

Roadmap step 2 of plan v1.2: Tor becomes real infrastructure on the egress
node, and Ghost mode is enforced **server-side** — a ghost peer can reach Tor
and nothing else, no matter what its client does.

Status: **live-verified 2026-09-06** on orion-egress-1 (DigitalOcean,
203.0.113.10): the table below was executed as written — ghost peer exited
via a Tor exit (185.220.101.21) on both HTTP and HTTPS, internet ICMP was
dropped, DNS to any resolver was captured by DNSPort (answers via Tor), and
`.onion` names automapped into 10.192.0.0/10. Live fixes folded back into the
roles: Debian 13's tor is multi-instance (`tor@default`, not the `tor.service`
/bin/true master) and `ExitPolicy` syntax is `reject *:*` (space-separated).

## What ships

**`ansible/roles/tor_egress`** — Tor as a client only:
- `ClientOnly 1` + `ExitPolicy reject:*:*` — the node never relays, so abuse
  complaints land on random Tor exits, not on the hosting provider.
- `SocksPort 127.0.0.1:9050` — loopback only, for node-local testing.
- `TransPort 10.66.0.1:9040` and `DNSPort 10.66.0.1:5353` — bound to the
  tunnel gateway IP; reachable only from inside the tunnel.
- `AutomapHostsOnResolve` — tunnel clients can resolve `.onion` names.

**nftables (table `orion_fw`)** — per-peer mode enforcement:
- prerouting: ghost-pool TCP → `redirect to :9040`, ghost-pool UDP/53 →
  `redirect to :5353`.
- input: only TransPort/DNSPort accepted from the ghost pool.
- forward: the ghost pool still has **no** accept rules — anything that
  didn't match a redirect (QUIC, other UDP, ICMP, IPv6) is dropped. The
  fast/double paths are unchanged.

**Ports 9040/5353 are contractual** — torrc and the nftables rules both
hardcode them, with comments pointing at each other. Don't change one side.

## Live verification (on the rented node)

Provision per phase-1.md, then create two peers:
`scripts/new-peer.sh test-fast fast` and `scripts/new-peer.sh test-ghost ghost`,
paste both into `host_vars/egress-1.yml`, re-run the playbook. From the client
machine, bring up one profile at a time:

| Check | Fast peer (10.66.0.10) | Ghost peer (10.66.0.90) |
|---|---|---|
| `curl https://api.ipify.org` | your home IP (NAT) | a Tor exit IP |
| `ping -c1 -W2 1.1.1.1` | replies | **dropped** |
| `getent hosts example.com` | via tunnel | captured by DNSPort (works) |
| `getent hosts <random>.onion` | n/a | 10.192.x.x (automap) |
| `nmap -sU -p 443 1.1.1.1` (or nc) | QUIC port **dropped** | **dropped** |

Node-side sanity: `curl --socks5-hostname 127.0.0.1:9050
https://api.ipify.org` must also show a Tor exit; `systemctl status tor`
should show bootstrapped 100%.

## Notes

- Tor comes from Debian's stable repo (security-maintained). The Tor Project
  APT repo is an optional later switch for fresher versions.
- Circuit rotation stays Tor's own (~10 min, per §5.4). The control port stays
  loopback-only; client-side NEWNYM-on-session-start arrives with the Ghost
  UX in roadmap step 5.
