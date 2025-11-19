# Tools Playbook for Agents

This directory hosts the guard-rail scripts that keep capability metadata in a
known-good state. When in doubt, study `capabilities_adapter.sh` (source of truth) and
`validate_capabilities.sh` (consumer/validator) to understand how the tooling is
wired together.

`generate_probe_coverage_map.sh` inspects the live probes and emits a
capability→probe coverage map (matching `docs/data/probe_cap_coverage_map.json`).
Run it manually when you add probes or capabilities to refresh the coverage map
before updating docs.

Before changing or adding tooling:
- Mirror the existing safety posture: every script sets `set -euo pipefail`,
  resolves `repo_root`, and fails fast if prerequisites are absent.
- Reuse the adapters that already normalize data. For example,
  `capabilities_adapter.sh` is the only supported way to read
  `schema/capabilities.json`; parsing the file directly can lead to stale or
  inconsistent semantics.
- Ship hermetic behaviors. Store helper jq/awk/sed snippets inline (as the
  adapter does) so contributors can audit the script without hunting external
  files.
- Validate inputs early and emit actionable errors (include file paths the way
   the current tools do).
- Document your intent at the top of the script with a guard-rail summary so
  future agents understand the blast radius and know which invariants the tool
  defends.

