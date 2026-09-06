# Project Orion — plan review (2026-09-06)

> **UPDATE (2026-09-06):** The user accepted the recommendations below; all issues are
> resolved in **plan v1.2** — see `docs/personal-vpn-system-design.md` §0 Changelog,
> which maps each finding to its fix. This review is kept as the historical record.

**Plan reviewed:** docs/personal-vpn-system-design.md (v1.1)

## Verdict: 7/10

Right core instinct — anchoring anonymity on Tor instead of on servers you own —
executed with unusual honesty (§8), but weakened by internal contradictions, two
actively counterproductive features, and an underestimated client.

| Area | Score | Note |
|---|---|---|
| Threat-model honesty | 9/10 | §8 is the best part of the doc |
| Core architecture | 6.5/10 | Tor egress right; double-hop under-justified; UDP unspecified |
| OpSec | 8/10 | Monero/njal.la/SimpleLogin sound; provider choice contradicts payment |
| Internal consistency | 6/10 | "Tor core, not optional" vs roadmap order vs geo features |
| Client feasibility | 5/10 | Kill switch + auto-switcher scope underestimated |

## What's genuinely right

1. **Tor as mandatory egress is the single best decision in the doc.** A chain of two
   servers you own shares one operator, one payment identity, one jurisdiction pattern —
   by itself it's a longer chain, not a bigger anonymity set. Routing the last mile
   through Tor's relay network is what actually buys that. Most self-hosted VPN designs
   never realize this.
2. **§8 "Honest limits"** correctly names timing correlation and the global passive
   adversary as unsolved, and refuses to sell the design as untraceable. Rare and
   valuable — keep this section as the doc's tone-setter.
3. **SimpleLogin instead of automating burner-email signups** (§7) — correct call; the
   doc even explains why routing around CAPTCHAs is wrong. Good judgment.
4. **§6.1 identity hygiene** — Tails/Whonix tiering and the stylometry warning are
   sophisticated; stylometry has deanonymized people who did everything else right.
5. **Ansible provisioning** — IaC makes "reimage periodically" (§6) actually cheap and
   repeatable instead of aspirational.

## Critical issues (fix before building)

### 1. The 10–15 min NEWNYM rotation is counterproductive
`SIGNAL NEWNYM` doesn't guarantee a new exit IP — it clears the circuit pool, and Tor
still reuses circuits per destination for ~10 min and rotates them on its own schedule.
So the timer adds little, while the *observable behavior* it creates is harmful: a
session whose exit IP changes every 10 minutes is itself rare and linkable, and
mid-session exit changes trigger Cloudflare CAPTCHA storms — which push the user to log
in, i.e. to leak identity through the app layer the design is trying to protect.
**Recommendation:** delete the timer. Rotate on session boundaries (ghost-mode toggle),
let Tor's own circuit lifetime do the work.

### 2. UDP/QUIC/ICMP handling is unspecified — a silent leak-or-break decision
TransPort is TCP-only; DNSPort covers DNS. §3.3 says "redirects all outbound TCP" and
stops there. The two possible behaviors are opposite:
- UDP dropped → QUIC/HTTP3, WebRTC calls, games, ping all break (acceptable, but say so
  and block QUIC explicitly so browsers fail over to TCP fast);
- UDP NATed out the VPS → traffic exits from the Toronto IP **outside Tor** — a real
  leak that correlates exit IP with destination.
**Recommendation:** explicit default-DROP policy on the egress node for everything not
bound for TransPort/DNSPort, including UDP 53 (so DNS can't route around DNSPort) and
UDP 443 (QUIC). Same fail-closed rule on the client: if Tor or any hop dies, the only
permitted traffic is the tunnel itself.

### 3. Provider choice contradicts the payment method
§4 picks Vultr or DigitalOcean; §6 pays with Monero. Vultr/DO don't accept it, and both
are US companies with the data-sharing posture that implies. **Recommendation:** pick
hosts where §6 is actually executable — Njalla (also registers domains), 1984 Hosting,
BuyVM — and re-derive the ~$10–12/mo estimate there. One genuine plus worth adding to
the doc: the egress box runs a Tor *client*, not a relay/exit, so abuse complaints land
on random Tor exits rather than your provider — your VPS stays quiet.

### 4. The double-hop is under-justified
Entry (Singapore) sees your real IP; egress (Toronto) sees only Tor-guard traffic. Both
nodes are yours, on one account, billed from one identity. The chain defends mainly
against the entry's hosting provider colluding — but the billing identity already links
you to both boxes, and §6 concedes the payment trail is only "partially solvable".
What the second hop *does* buy: it hides the fact that you use Tor from the Singapore
datacenter's upstream, and it separates your Tor guard from your home IP. If that's in
your threat model, keep it and say so; if not, client → AmneziaWG(entry) → Tor gives
the same anonymity set with ~200ms less transit. **Recommendation:** keep double-hop as
an option, justify it explicitly, and don't make it a prerequisite for Tor egress (see
roadmap).

### 5. Geo-unblock and Tor egress are different products
Tor exits are random relays worldwide — you don't control geography. Smart DNS / SNI
proxy (§3.1) exists to *pin* geography. The doc carries both as features without a mode
matrix. **Recommendation:** define three modes explicitly:
- **Fast** — single WG hop, geo/Smart-DNS available, no Tor;
- **Double** — chained hops, no Tor, geo available via egress;
- **Ghost** — mandatory Tor, geo features hard-disabled, split tunneling hard-disabled,
  kill switch fail-closed.
Also resolve the wording tension: §3.4 calls Tor "core, not optional" while the roadmap
ships it at step 5 and geo tools at step 7. If Tor is core, Ghost is the product and the
rest is convenience modes around it.

### 6. Browsing over TransPort makes you a fingerprint duck
A regular Chrome/Firefox behind TransPort presents a unique fingerprint to a hostile
audience that already assumes Tor users are worth fingerprinting — timezone/locale
matching (§6) can't fix canvas/fonts. The doc's own §6.1 answers this: for web, use Tor
Browser (or Whonix). **Recommendation:** reposition TransPort as what it's actually
good at — forcing *non-browser* TCP apps (CLI tools, git, mail clients, IMAP/SMTP) through
Tor transparently — and let the browser story default to Tor Browser in Ghost mode.

## Features to cut or reposition

- **Packet timing jitter** (§3.1 ghost mode): homebrew mixnet against a passive
  observer — the doc's own §8 concedes timing correlation is unsolved. It costs latency
  and buys ~nothing. Cut it or label it cosmetic.
- **Latency-scored auto-switcher across a pool of nodes** (§3.1): the architecture has
  exactly two nodes. Failover between one entry and one egress is a reboot script, not
  an auto-switcher. Defer until a node pool exists.
- **"No persistent logging — WireGuard logs routed to tmpfs"** (§3.2): WireGuard keeps
  no flow logs by design; the real log sources are journald, sshd, unattended-upgrades.
  Implement via Ansible: `journald Storage=volatile`, and say that's the mechanism.
- **WebRTC "leak protection"** (§3.6): with UDP dropped, WebRTC doesn't leak — it just
  fails. State that instead of implying a mitigation.

## Missing pieces

- **A threat model section up front.** §8 is honest about limits but the doc never says
  who the adversary is. ISP/corporate surveillance? Doxxers? Platforms? The answer
  changes whether Ghost is the default or an occasionally-used mode.
- **Fail-closed policy statement** — default-DROP everywhere (client + egress), explicit
  allows only, including on Tor daemon crash.
- **Key lifecycle** — WG key rotation per node/client, and rotation of AmneziaWG junk
  parameters (static obfuscation params age as AmneziaWG gets fingerprinted).
- **SimpleLogin caveat** — all aliases forward to one inbox, which makes per-*session*
  aliases linkable by the provider. Prefer per-*identity* aliases, ideally to a
  catch-all on your own domain (registered via Njalla, WHOIS-shielded).
- **macOS client reality** — AmneziaWG on macOS means a Network Extension with
  entitlements; the kill switch per OS is nftables (Linux) / pf (macOS) / WFP (Windows).
  Windows WFP callouts are a project of their own — scope the client per-OS honestly.
- **Clock/consistency details** — browser DoH vs DNSPort resolution order; explicit
  QUIC block list.

## Suggested roadmap reorder

Ship the privacy core before the redundancy:

1. Single-hop WireGuard + GUI + kill switch (fail-closed nftables on the client)
2. Tor TransPort + DNSPort on the egress node, QUIC/UDP drop policy
3. AmneziaWG obfuscation on that hop
4. Chain entry → egress (double-hop) as an *option*, with latency measured honestly
5. Ghost mode toggle unifying the above
6. Split tunneling + Smart DNS (non-Ghost modes only) + polish

Rationale: if the double-hop proves too slow for daily use, the core privacy product
(obfuscated hop + Tor egress + fail-closed client) has already shipped.

## One-line summary

Anchor on Tor (already done, and it's the right call), delete the NEWNYM timer and the
jitter, specify the DROP rules, reconcile providers with the payment story, define the
mode matrix — and this goes from a 7 to a 9 as a personal project.
