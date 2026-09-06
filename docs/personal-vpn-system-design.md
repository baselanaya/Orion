# Personal privacy VPN — system design (v1.3)

> **v1.3 — 2026-09-06.** Payment stance changed to regular fiat — providers
> re-picked, attribution trade-off made explicit (§0, §6, §10).
> **v1.2 — 2026-09-06.** Full revision after independent review (`REVIEW.md`). The
> Tor-anchored core of v1.1 is kept; everything around it was corrected, justified,
> or cut. See §0 Changelog for the issue-by-issue map.

## 0. Changelog

v1.2 → v1.3 (2026-09-06, user decision):

- **Payment switched to regular fiat** (card / PayPal, no crypto). Providers
  re-picked on price/latency/reliability instead of payment privacy; §6 table
  replaced. Debit-card constraint (same day): Vultr's card checks reject
  debit — DO/Contabo/OVH take debit directly, Vultr works via PayPal.
- §7 rewritten for the fiat reality; §10 states the attribution trade-off
  explicitly instead of "partially solvable".
- Architecture, modes, and the phase 1–2 implementation are unchanged — Tor
  egress carries the anonymity weight toward destinations, not the billing
  trail.

v1.1 → v1.2:

- **Threat model added** (§2) — adversaries in and out of scope, and the defaults they dictate.
- **NEWNYM rotation timer removed** (§3.4, §7) — Tor's own circuit lifetime is the rotation policy; rotation happens on session boundaries only.
- **UDP/QUIC/ICMP handling specified** (§3.3, §5.6, §5.7) — explicit default-DROP policy; the silent leak-or-break decision is now "break, and break loudly".
- **Hosting reconciled with payment** (§6) — Vultr/DO replaced with privacy-coherent providers that actually accept Monero; costs re-derived (~$13–18/mo).
- **Double-hop demoted from default to option** (§3, §5.2) — with an explicit statement of what it buys and a latency test that decides if it stays.
- **Mode matrix added** (§4) — Fast / Double / Ghost; geo features and Tor are now explicitly separate products.
- **TransPort repositioned** (§5.4) — for non-browser TCP apps; Ghost-mode web browsing is Tor Browser's job.
- **Packet timing jitter cut** (§5.1) — theater per the doc's own honest-limits section.
- **Auto-switcher deferred** (§5.1) — meaningless across two nodes; failover script until a pool exists.
- **Logging claim made real** (§5.2) — journald `Storage=volatile` via Ansible instead of a fictional "WireGuard logs to tmpfs".
- **Fail-closed policy stated** (§5.7) — default-DROP everywhere, explicit allows only.
- **Client scope made honest** (§5.1) — kill-switch mechanics per OS; Linux first; Windows/macOS are their own milestones.
- **SimpleLogin reworked** (§7) — per-identity aliases on an owned domain, not per-session aliases forwarding to one linkable inbox.
- **Roadmap reordered** (§9) — Tor egress ships before double-hop chaining.

## 1. Goal

A self-hosted, obfuscated, Tor-anchored personal VPN: WireGuard-family tunnels with
DPI resistance, egress through Tor's relay network when it matters, direct egress when
it doesn't — with no commercial VPN provider anywhere in the trust chain.

Not goals: competing with commercial VPNs on throughput (Ghost mode is slow by
design), or defending against adversaries in §2's out-of-scope list.

## 2. Threat model

**Adversaries in scope:**

| Adversary | Defended by |
|---|---|
| Your ISP (visibility, throttling, DPI of WireGuard handshakes) | AmneziaWG obfuscation + Tor anchoring |
| Local network / WLAN operator | same |
| IP-based geo-restriction and IP-reputation blocking | Fast/Double egress region; Ghost random exits |
| Opportunistic tracking / doxxing via IP address | Ghost mode's Tor anonymity set |
| VPS provider records | attributable by choice (fiat payment, §6) — accepted; what's hidden is the *content*, not the billing |

**Adversaries explicitly out of scope** (per §10, and Tor's own threat model):

- Global passive adversary / traffic-confirmation attacks
- Endpoint compromise (malware on your machine) — see §8 for discipline, not guarantees
- Legal coercion of providers at scale
- Stylometric deanonymization — §8 mitigates, nothing personal-scale defeats it

**Defaults this dictates:** Fast/Double are the daily modes; Ghost is an opt-in heavy
mode, not the default. "Untraceable" is not a claim this system makes; §10 says what
it does claim.

## 3. Architecture overview

```
Desktop client (mode selected in UI)
      |
      |  AmneziaWG — obfuscated WireGuard (junk packets defeat handshake DPI)
      v
Entry node  ─────────────────────────────────────┐
      |                                          |
      |  second WG hop                           |  Fast mode: direct NAT egress
      |  (Double / Ghost only)                   |  from the entry node itself
      v                                          |
Egress node  <───────────────────────────────────┘
      ├── Ghost: Tor TransPort/DNSPort → Tor 3-hop circuit → destination
      └── Fast/Double: direct NAT egress → destination
```

- **Fast**: only the entry node is used; it NATs directly to the internet.
- **Double**: entry forwards over a second WireGuard tunnel to the egress node; direct NAT out.
- **Ghost**: same path as Double, but the egress node force-redirects everything into Tor.

Tor usage is hidden from the entry node's upstream in Ghost mode; the entry's
operator/ISP sees only obfuscated WireGuard. The egress node runs a Tor *client* —
actual exits are random Tor relays worldwide, which is the entire point.

## 4. Modes

| | Fast | Double | Ghost |
|---|---|---|---|
| Path | client → entry → internet | client → entry → egress → internet | client → entry → egress → **Tor** → internet |
| Tor | no | no | mandatory, enforced server-side |
| Obfuscation | AmneziaWG | AmneziaWG | AmneziaWG |
| Geo control | entry region | egress region | none — random exits |
| Smart DNS / split tunneling | allowed | allowed | **hard-disabled** |
| Relative latency | baseline | + inter-region RTT | + Tor (2–5×, variable) |
| Use for | daily browsing, geo, speed | separating Tor usage from home IP; guard separation | anonymous sessions; non-browser TCP apps |

Rules encoded in this table:

- Ghost disables split tunneling and Smart DNS at the firewall level, not just the UI —
  the egress node's per-peer policy makes bypass structurally impossible (§5.3).
- Geo features only exist in modes without Tor. Tor picks your exit country at random;
  a "geo" feature inside Ghost is a contradiction, so it doesn't ship there.

## 5. Components

### 5.1 Desktop client (Tauri)

- **Process architecture**: unprivileged Tauri UI + a small privileged helper daemon
  (root/Administrator) that owns tunnel lifecycle and firewall rules. The helper is
  deliberately tiny and auditable; all privileged operations live there, exposed over
  local IPC only.
- **Kill switch (fail-closed)**: firewall rules install *before* the tunnel comes up
  ("block everything, then allow"), and are removed only after an *intentional*
  disconnect. Rules persist across app or helper crashes; the documented emergency
  unlock command is the only manual escape. Semantics: allow loopback; allow traffic to
  the entry node's endpoint; allow everything else only via the tunnel interface.
- **Per-OS mechanics** — honestly scoped:
  - Linux: nftables (v1 target — this is the daily-driver platform).
  - macOS: pf via helper for v1; Network Extension is the hardened path and its own
    milestone (entitlement + signing).
  - Windows: WFP filters via a service — a substantial standalone effort, scheduled
    late (§9), not assumed for free by a "cross-platform" label.
- **Split tunneling**: per-app / per-route bypass, **hard-disabled in Ghost** (§4).
- **Tor Browser integration**: in Ghost mode the client does not pretend a regular
  browser is anonymous over TransPort; it offers to configure/launch Tor Browser for
  web traffic (§5.4). Ghost's audience for TransPort is *other* TCP apps.
- **Removed from v1.1**: packet-timing jitter (cosmetic against timing correlation,
  §10; costed latency for nothing); latency-scored auto-switcher across a node pool
  (there are two nodes — v1 ships per-mode manual selection plus a reconnect script;
  an auto-switcher returns only when a pool exists).

### 5.2 Entry node

- Minimal hardened Debian, AmneziaWG server terminating the client tunnel.
- **Logging**: WireGuard keeps no flow logs by design; the real log sources are
  journald, sshd, and unattended-upgrades. Ansible sets `journald Storage=volatile`
  (tmpfs-backed, gone at reboot), disables persistent journal, and keeps sshd quiet.
  "No persistent logging" is now an implemented mechanism, not a hope.
- **Mode routing**: Fast-mode peers are NATted out directly; Double/Ghost peers are
  forwarded over the second WireGuard hop. Policy is per-WG-peer on the server side,
  so a buggy or compromised client cannot talk to the wrong egress.
- **Location**: the provider region with the best RTT to you, in an acceptable
  jurisdiction. (The v1.1 "Singapore" choice was an artifact of Vultr/DO's region
  lists; with privacy-coherent hosts, see §6 for what's actually available.)

### 5.3 Egress node (renamed from "exit node")

It doesn't exit — Tor's random relays do. This box terminates the second hop and runs
a Tor **client** with:

- `TransPort` on 9040, `DNSPort` on 5353, `SocksPort` for local apps.
- nftables policy (sketch — final ruleset in the Ansible role):

```
# Ghost peers (source range):
tcp syn  → redirect to 9040        # TransPort takes all TCP
udp dport 53 → redirect to 5353    # DNSPort takes all DNS
drop udp dport { 443, 53 }         # belt & braces: QUIC + any DNS that missed the redirect
drop ip6                           # no IPv6 on the Tor path
drop                               # default — nothing else leaves

# Fast/Double peers:
NAT out directly; drop udp dport 443 (force HTTP/3 fallback to TCP); drop ip6 per client policy

# The host's own egress:
whitelist only (SSH from admin sources, WireGuard, NTP, package updates)
```

- **Why the rename matters**: as a Tor client (never a relay or exit), abuse complaints
  land on random Tor exits, not on the hosting provider. The VPS stays quiet. This is
  also a standing rule: these boxes never run Tor relays.

### 5.4 Tor integration (Ghost core)

- Tor's own circuit lifetime **is** the rotation policy: circuits rotate roughly every
  10 minutes, with per-destination stickiness that keeps your sessions stable.
- `NEWNYM` is sent **only** on explicit Ghost-session start, and the client documents
  that it clears the circuit pool without guaranteeing a new exit IP. There are **no
  timers** — v1.1's 10–15 minute rotation was counterproductive: mid-session exit
  changes are themselves a rare, linkable behavior, and they trigger CAPTCHA storms
  that push users toward logging in (app-layer identity leak through the exact window
  Ghost exists to protect).
- **Browser policy**: Ghost-mode web browsing uses Tor Browser (or Whonix, §8).
  Timezone/locale matching cannot fix canvas/font fingerprinting; a regular browser
  over TransPort is a unique duck on the Tor network.
- **TransPort's real job**: transparently forcing *non-browser* TCP apps — git, CLI
  tools, mail clients, anything TCP — through Tor.
- Tor is "core" to Ghost mode specifically. Fast/Double modes exist because Tor is the
  wrong tool for speed and geo-control; the mode matrix (§4) is the honest reconciliation
  of v1.1's "Tor is not optional" with its own geo feature list.

### 5.5 Obfuscation layer

- **AmneziaWG on every hop, always** — junk packets and modified handshake defeat
  passive WireGuard DPI. Even Fast mode is obfuscated.
- **Fallback** if AmneziaWG gets blocked in a region: **VLESS + Reality** (XTLS Vision)
  via sing-box/xray on the entry node, borrowing a real frontend site's TLS fingerprint.
  Shadowsocks-2022 as the lightweight alternative. (v1.1's VMess/Trojan suggestion
  dropped — VMess's crypto design is dated; Reality is the current state of the art.)

### 5.6 DNS & leak protection

- Client resolver goes through the tunnel: Ghost → egress `DNSPort`; Fast/Double → DoH
  over the tunnel to a resolver of choice. Browser-local DoH is fine *because* it is
  still inside the tunnel — it cannot bypass the chain.
- Egress drops UDP 53 from Ghost peers (DNS cannot route around DNSPort) and UDP 443
  in all modes (QUIC cannot route around TransPort / force browsers to TCP).
- IPv6: dropped at the client firewall by default (rather than the v1.1 blunderbuss of
  disabling it OS-wide — the user can still disable globally if preferred), and dropped
  on the egress forward path. No RA/ND leak-around.

### 5.7 Fail-closed policy (global invariant)

- **Client**: default-DROP; explicit allows only (loopback, entry endpoint, tunnel
  interface). Rules precede the tunnel and outlive crashes (§5.1).
- **Egress node**: Ghost peers can reach Tor and nothing else — enforced by source-range
  firewall policy, not by client cooperation. A daemon failure drops packets; it never
  falls back to direct NAT.
- **Tor down** ⇒ nothing egresses in Ghost mode. The client surfaces a degraded state
  rather than silently degrading to direct egress.

## 6. Server provisioning

**Providers (v1.3 — regular payment, no crypto; attribution accepted, see §10):**

| Role | Provider | Regions | Price | Payment |
|---|---|---|---|---|
| Egress (rent now) | **BuyVM / FranTech** | Las Vegas, New Jersey, Miami, Luxembourg | ~$4–6/mo, smallest KVM slice | card / PayPal |
| Egress alternative (debit-friendly) | **DigitalOcean**, **Contabo**, **OVHcloud** | many — incl. Singapore, Toronto, Frankfurt | $4/mo (DO), ~€5 (Contabo, OVH) | debit card / PayPal / SEPA & Maestro (OVH) |
| Egress (Vultr, PayPal route) | **Vultr** | many, incl. Singapore, Toronto | ~$2.50–6/mo | PayPal linked to debit — its card checks reject debit/prepaid |
| Egress alternative (EU) | **Hetzner** (post-2026 price hikes) | Germany, Finland, US | ~€4.50–6/mo | card / SEPA |
| Entry (roadmap 4, later) | same set — optimize for lowest RTT to you | — | ~$4–6/mo | card |
| Domain (Smart DNS / aliases, later) | any registrar with WHOIS privacy — Njalla also takes card | — | €10–15/yr | card |

- Pick the egress region for the geo behavior you want from Fast/Double mode;
  pick the entry (later) for latency. Tor egress ignores geography by design.
- **Two separate provider accounts, one per node** — still worth it: one
  account is one subpoena revealing both hops. Realistic total: **~$8–12/mo**.
- Prices shift; verify the current tier at signup. The node needs almost
  nothing: 512 MB–1 GB RAM, 10 GB disk, Debian 12, unrestricted outbound.
- **Ansible playbook** (idempotent, re-runnable): hardened Debian minimal; key-only
  SSH on a non-standard port; fail2ban; journald `Storage=volatile`; unattended-upgrades;
  nftables baseline; AmneziaWG; Tor (egress). **Periodic reimage = re-running the
  playbook**, which makes §7's "reimage periodically" cheap and honest instead of
  aspirational. Quarterly cadence.
- **Key lifecycle**: client and node WireGuard keypairs regenerate at each reimage or
  quarterly, whichever comes first; AmneziaWG junk parameters rotate on the same
  schedule (both sides, costs one reconnect). Keys live in the client helper's
  credential storage — not plaintext dotfiles.

## 7. Operational security checklist

Network-layer anonymity is undone by app-layer identity leaks. With fiat
payment (v1.3) the billing trail is a known, accepted link — the checklist
keeps the parts that still earn their keep:

- Use a dedicated email alias for the hosting account (SimpleLogin / catch-all
  on your own domain) — never your main inbox — and a unique generated password.
- **Don't log into personal accounts in Ghost mode** — those requests carry your
  identity regardless of exit IP.
- Match browser timezone/locale to the exit's region *when a regular browser is
  unavoidable in Fast/Double modes*; in Ghost mode use Tor Browser and skip this —
  resisting fingerprint *normalization* inside Tor Browser is the job, and Tor Browser
  already uniformizes.
- **Rotation policy (corrected)**: rotate identities on *session boundaries*; let Tor
  rotate circuits itself (~10 min); no timers.
- **Reimage quarterly** via the §6 playbook.
- **Burner email (corrected)**: no automated throwaway signups — and also not
  per-*session* SimpleLogin aliases (every alias forwards to one inbox, which makes the
  alias set linkable by the provider). Instead: **per-identity aliases** via
  **SimpleLogin or a catch-all on your own domain** (registered through Njalla, where
  WHOIS is shielded by the registrar's design). Same "here's your address" UX in the
  client, bound to identities rather than sessions.

### 7.1 Identity hygiene (endpoint hardening)

Unchanged from v1.1 — this section was right:

- For high-risk sessions, boot **Tails** — stateless, Tor by default.
- Or run **Whonix** — Tor gateway split from the workstation across two VMs, so an
  app-level exploit can't see your real IP.
- Never cross-use this identity's hardware/VM/profile with your real life —
  stylometry alone has deanonymized people who got everything else right.

## 8. Session identity

- **Ghost-session start** = NEWNYM once (documented as pool-clear, not IP guarantee) +
  SimpleLogin/domain-alias for the identity in play + Tor Browser launched or pointed
  at the right isolation. **Ghost-session end** = Tor's own circuit retirement. Nothing
  in between runs on a timer.

## 9. Roadmap

Reordered: the privacy core ships before the redundancy.

1. **Foundation** — Ansible playbook; egress node with Fast path (single-hop plain
   WireGuard + NAT + QUIC drop); Linux client skeleton (Tauri + root helper) with the
   fail-closed nftables kill switch.
2. **Tor core online** — TransPort/DNSPort, per-peer mode firewall groups, Ghost
   policy enforced server-side, DNS/QUIC/IPv6 drops verified.
3. **Obfuscation** — AmneziaWG on the hop + junk-parameter lifecycle.
4. **Double hop** — chain entry → egress; measure latency honestly; keep only if it
   earns its RTT (it's optional by design).
5. **Ghost mode UX** — mode toggle unifying client policy; Tor Browser integration;
   per-identity alias flow.
6. **macOS + Windows clients** — pf → Network Extension; WFP service. (Each is a real
   milestone, not a checkbox.)
7. **Convenience layer** — split tunneling, Smart DNS (non-Ghost modes only),
   auto-switcher (only once a node pool exists), polish.

Rationale: if the double-hop proves too slow for daily use, or if Windows WFP turns
into a swamp, steps 1–3 already deliver the core product: obfuscated hop + Tor egress +
fail-closed client.

## 10. Honest limits — read this before relying on it

Unchanged in substance from v1.1, because it was correct — now consistent with the
design (jitter is gone; rotation is session-boundary-based):

- **Payment/hosting trail** — with fiat payment (v1.3 decision), provider
  records attribute the nodes to you: accepted up front. What the provider can
  see is that you run a node and when you connect to it — not what passes
  through (obfuscated WireGuard, Tor-encrypted egress). Ghost mode's anonymity
  toward destinations is unaffected; what's given up is anonymity toward your
  own hosting provider and anyone who can compel them.
- **Endpoint compromise** — solvable with discipline (§7.1); nothing here defends a
  compromised endpoint.
- **Traffic timing correlation** — unsolved at personal scale; real mitigations (mixnets
  like Nym) cost more latency than daily use tolerates. This design does not pretend
  otherwise, and no longer carries cosmetic anti-correlation features.
- **Global passive adversary** — unsolvable by design; Tor's threat model explicitly
  excludes it.

If your actual threat model includes nation-state adversaries, "untraceable" is not an
achievable target for any personal project, this one included.

What this design achieves: strong protection against your ISP, local network
operators, casual surveillance, DPI-based blocking, and IP-based geo-restriction —
plus, in Ghost mode, a real Tor-sized anonymity set — which covers the large majority
of legitimate personal privacy needs.

## 11. Sources (provider facts, checked 2026-09-06)

- Njalla — pricing & crypto: <https://njal.la/pricing/>, <https://njal.la/servers/>, <https://kycnot.me/service/njalla>
- 1984 Hosting — pricelist & XMR: <https://1984.hosting/product/pricelist/>, <https://kycnot.me/service/1984-hosting>
- BuyVM/FranTech — plans & crypto terms: <https://buyvm.net/kvm-vps/>, <https://buyvm.net/terms-of-service/>
- Reserve options: <https://bithost.io/monero-vps/>, <https://extravm.com/crypto-vps/>
