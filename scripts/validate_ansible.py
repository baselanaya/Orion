#!/usr/bin/env python3
"""Static validation for the Ansible layer (no live node needed).

Checks, in order:
  1. every YAML file under ansible/ parses
  2. every role template renders under strict-undefined with the union of
     role defaults + sample values (catches variable typos)
  3. the rendered nftables ruleset is written to /tmp for `nft -c` parse

Run with the project's pinned tooling:   uv run python scripts/validate_ansible.py
"""

from __future__ import annotations

import glob
import os
import sys

import jinja2
import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

CTX = {
    "ansible_managed": "ansible-validated",
    "ansible_default_ipv4": {"interface": "ens3"},
    "orion_ssh_port": 22,
    "orion_ssh_keep_port_22": True,
    "orion_wg_port": 4500,
    "orion_wg_iface": "awg0",
    "orion_awg_jc": 4,
    "orion_awg_jmin": 50,
    "orion_awg_jmax": 300,
    "orion_awg_s1": 87,
    "orion_awg_s2": 42,
    "orion_awg_h1": "1123214321-1143214321",
    "orion_awg_h2": "987654321-997654321",
    "orion_awg_h3": "765432109-775432109",
    "orion_awg_h4": "654321098-664321098",
    "orion_wan_interface": "ens3",
    "orion_vpn_subnet": "10.66.0.1/24",
    "orion_vpn_gateway_ip": "10.66.0.1",
    "orion_vpn_prefix": 24,
    "orion_pool_fast": "10.66.0.10-10.66.0.49",
    "orion_pool_double": "10.66.0.50-10.66.0.89",
    "orion_pool_ghost": "10.66.0.90-10.66.0.129",
    "orion_hop_port": 4501,
    "orion_hop_address": "10.66.255.2/30",
    "orion_hop_peer_ip": "10.66.255.1/32",
    "orion_hop_fast": "10.66.1.10-10.66.1.49",
    "orion_hop_double": "10.66.1.50-10.66.1.89",
    "orion_hop_ghost": "10.66.1.90-10.66.1.129",
    "orion_hop_private_key": "HOPKEYx" * 6,
    "orion_hop_peer_pub": "HOPPUBx" * 6,
    "orion_hop_endpoint": "203.0.113.10:4501",
    "orion_hop_fwmark": "4502",
    "orion_entry_fast": "10.66.1.10-10.66.1.49",
    "orion_entry_double": "10.66.1.50-10.66.1.89",
    "orion_entry_ghost": "10.66.1.90-10.66.1.129",
    "orion_peers": [
        {"name": "laptop", "mode": "fast", "ip": "10.66.0.10", "pubkey": "PUBKEYx" * 8},
    ],
    "orion_server_private_key": "PRIVKEYx" * 6,
}


def ansible_bool(v) -> bool:
    if isinstance(v, bool):
        return v
    return str(v).strip().lower() in ("yes", "true", "1", "on")


def main() -> int:
    failures = 0

    yamls: list[str] = []
    for dirpath, _, files in os.walk(os.path.join(ROOT, "ansible")):
        yamls.extend(
            os.path.join(dirpath, f) for f in files if f.endswith((".yml", ".yaml"))
        )
    for path in sorted(yamls):
        rel = os.path.relpath(path, ROOT)
        try:
            yaml.safe_load(open(path))
            print(f"YAML ok    {rel}")
        except Exception as e:  # noqa: BLE001 - report and continue
            failures += 1
            print(f"YAML FAIL  {rel}: {e}")

    env = jinja2.Environment(
        undefined=jinja2.StrictUndefined, keep_trailing_newline=True
    )
    env.filters["bool"] = ansible_bool

    rendered_nft = None
    for path in sorted(
        glob.glob(os.path.join(ROOT, "ansible/roles/*/templates/*.j2"))
    ):
        rel = os.path.relpath(path, ROOT)
        try:
            out = env.from_string(open(path).read()).render(**CTX)
            print(f"JINJA ok   {rel}")
            if "nftables.conf" in rel:
                rendered_nft = out
        except Exception as e:  # noqa: BLE001
            failures += 1
            print(f"JINJA FAIL {rel}: {e}")

    if rendered_nft:
        out_path = "/tmp/rendered-nftables.conf"
        with open(out_path, "w") as fh:
            fh.write(rendered_nft)
        print(f"rendered nftables -> {out_path} (run: nft -c -f {out_path})")

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
