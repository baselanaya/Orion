#!/usr/bin/env bash
# Re-apply the Orion playbook to all nodes: package upgrades, config drift,
# and any changes you made to the roles. Idempotent — safe to run anytime.
cd "$(dirname "$0")/.."
exec uv run ansible-playbook -i ansible/inventory.yml ansible/site.yml "$@"
