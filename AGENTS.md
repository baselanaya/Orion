# AGENTS.md — Project Orion

Self-hosted personal privacy VPN: Tauri desktop client + self-managed VPS nodes
(obfuscated AmneziaWG) + mandatory Tor TransPort egress on the egress node.

- Plan: docs/personal-vpn-system-design.md (v1.3 — fiat payments, no crypto)
- Review of v1.1 (7/10 + critique): REVIEW.md — all findings resolved in v1.2, see the
  plan's §0 Changelog for the issue-by-issue map
- Status: **phases 1-3 + packaging LIVE-VERIFIED** — see docs/phase-*.md runbooks
  and docs/design/client-design.md (NordVPN-style UI v0.5)
- The plan's §8 ("Honest limits") sets the honesty bar for all docs in this repo —
  don't claim capabilities the threat model doesn't buy.

## Working rules for agents

- Design gate: UI/design work is planned as text and confirmed by the user
  before any build. Design specs live in docs/design/client-design.md.
- Feature/security claims go through the plan's §2 threat model and §10 limits.
- Never commit live infrastructure details (node IPs, keys, host_vars content).

## Environment notes (per-machine; adapt to the host)

- Python must go through uv (`uv run ...`; .venv from `uv sync`; interpreter
  pinned in .python-version). ansible-core + community.docker come from the
  uv dev group.
- Rust: prefer the toolchain cargo (`~/.rustup/toolchains/*/bin/cargo`) —
  system shims can be broken on some setups.
- Validation: `uv run python scripts/validate_ansible.py`, then
  `uv run ansible-playbook --syntax-check`, then `nft -c -f` on rendered
  rulesets. Run the whole playbook — `--tags` with role names has silently
  skipped tasks before.
- AmneziaWG DKMS gotcha: it builds for the newest kernel — also install the
  RUNNING kernel's headers and `dkms install -k $(uname -r)`.
- Linuxdeploy (AppImage bundling) needs NO_STRIP=1 + APPIMAGE_EXTRACT_AND_RUN=1.
