# Tools Playbook for Agents

This directory hosts helpers for automated agents.

## Available tooling

- `adapt_capabilities.sh`: fast, simple reader for capability catalogs.
- `validate_contract_gate.sh`: canonical probe contract gate (static + dynamic checker) used by Probe Authors when creating new probes and by `probe-gate` / `bin/probe-contract-gate` under the test and CLI entry points.
- `sync_bin_helpers.sh`: rebuilds and syncs Rust helper binaries into `bin/`.
- `helpers.manifest.json`: source of truth for the helper binaries to sync into `bin/`.

## Modifying tooling
Before changing or adding tooling:
- Mirror the existing safety posture: every script sets `set -euo pipefail`,
  resolves `repo_root`, and fails fast if prerequisites are absent.
- Ship hermetic behaviors. Keep awk/sed/python snippets inline (as the adapter
  does) so contributors can audit the script without hunting external files.
  Probes themselves should not depend on external JSON tooling.
- Validate inputs early and emit actionable errors (include file paths the way
   the current tools do).
- Document your intent at the top of the script with a guard-rail summary so
  future agents understand the blast radius and know which invariants the tool
  defends.
- The static probe contract must stay portable (`/bin/bash 3.2` on macOS), silent on success, and deterministic. The Rust guard rails inherit the same expectations even though they run through Cargo.
