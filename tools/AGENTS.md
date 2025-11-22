# Tools Playbook for Agents

This directory hosts helpers for automated agents developing in the repo.

## Available tooling

- `adapt_capabilities.sh`: fast, simple reader for `capabilities.json`.
- `validate_contract_gate.sh`: static + dynamic checker used by Probe Authors when creating new probes.
- `resolve_paths.sh`: canonicalizes probe paths and exports the `resolve_probe_script_path`
  + `portable_realpath` helpers shared by the contract tools.
- `list_run_modes.sh`: canonical list + parser for supported run modes.
- `audits/INTERPRETERS.md`: AI agent prompts for audits.

## Modfiying tooling
Before changing or adding tooling:
- Mirror the existing safety posture: every script sets `set -euo pipefail`,
  resolves `repo_root`, and fails fast if prerequisites are absent.
- Ship hermetic behaviors. Keep awk/sed snippets inline (as the adapter does)
  so contributors can audit the script without hunting external files. jq is
  only used by the contract gate; probes themselves should not depend on it.
- Validate inputs early and emit actionable errors (include file paths the way
   the current tools do).
- Document your intent at the top of the script with a guard-rail summary so
  future agents understand the blast radius and know which invariants the tool
  defends.
- The static probe contract must stay portable (`/bin/bash 3.2` on macOS), silent on success, and deterministic. The Rust guard rails inherit the same expectations even though they run through Cargo.
