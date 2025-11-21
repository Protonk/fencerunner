# Tools Playbook for Agents

This directory hosts the remaining guard-rail helpers for capability metadata.
`capabilities_adapter.sh` stays as a legacy entry point for automated agents and
for shell callers that need to read `schema/capabilities.json`.

Keep capability updates in sync with the Rust guard rails and schema; prefer the
adapter for shell access instead of rolling ad-hoc parsers.

Before changing or adding tooling:
- Mirror the existing safety posture: every script sets `set -euo pipefail`,
  resolves `repo_root`, and fails fast if prerequisites are absent.
- Reuse the adapters that already normalize data. For example,
  `capabilities_adapter.sh` is the supported way for shell scripts to read
  `schema/capabilities.json`; Rust callers should reuse the capability index and
  avoid parsing the file directly in ad-hoc ways.
- Ship hermetic behaviors. Store helper jq/awk/sed snippets inline (as the
  adapter does) so contributors can audit the script without hunting external
  files.
- Validate inputs early and emit actionable errors (include file paths the way
   the current tools do).
- Document your intent at the top of the script with a guard-rail summary so
  future agents understand the blast radius and know which invariants the tool
  defends.
