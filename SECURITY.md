# Security Policy

## Supported version

- `main` (and the latest tagged release) — older releases receive no updates.

## Reporting a vulnerability

Open a **private security advisory** via GitHub's *Security → Advisories* tab on
this repository. Please include the affected component (helper daemon, node
roles, UI) and reproduction steps. Do not open a public issue for
vulnerability reports.

## What Orion claims — and what it does not

Orion protects your traffic in transit and enforces confinement server-side.
It does **not**:

- protect a compromised endpoint (malware on your machine defeats everything)
- defend against a global passive adversary / traffic-confirmation attacks
- erase the payment trail of your own VPS (fiat payment is an accepted,
  documented trade-off in the design doc, §10)

The full threat model, including open problems that no personal project
solves, is in [docs/personal-vpn-system-design.md §10](docs/personal-vpn-system-design.md).

## Design invariants (verified, not aspirational)

- Kill-switch rules are installed **before** the tunnel and live in the
  kernel: a crashed helper seals the machine (`locked_no_tunnel`), it never
  exposes it.
- Ghost peers are Tor-only **enforced server-side by firewall rules** — a
  compromised or buggy client cannot bypass the redirect.
- DNS cannot route around the tunnel (UDP 53 redirects to Tor's DNSPort in
  Ghost; QUIC/443-UDP and IPv6 are dropped on the egress path).
- Helper drops `PostUp`/`PreUp`/`PostDown`/`PreDown` lines from profiles —
  a poisoned profile cannot execute commands through wg-quick.
- Node logs are volatile (journald `Storage=volatile`) and nodes are meant to
  be re-imaged quarterly via the Ansible playbook.
