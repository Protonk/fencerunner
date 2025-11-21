#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Shared run-mode definitions for bash helpers.
#
# Goals:
# - Keep contract gate mode coverage aligned with the modes the harness cares
#   about (baseline, codex-sandbox, codex-full).
# - Allow overrides via PROBE_CONTRACT_MODES for tests or rapid experiments.
# -----------------------------------------------------------------------------
set -euo pipefail

# Canonical list of run modes codex-fence supports. Update here when adding a
# new mode so downstream bash helpers stay in sync.
codex_fence_run_modes() {
  printf '%s\n' "baseline" "codex-sandbox" "codex-full"
}

# Mode resolver for contract-gate tooling. Respects PROBE_CONTRACT_MODES when
# provided (space- or comma-separated), otherwise falls back to the canonical
# list above.
contract_gate_modes() {
  local override="${PROBE_CONTRACT_MODES:-}"
  if [[ -n "${override}" ]]; then
    printf '%s' "${override}" | tr ',' ' ' | tr -s ' ' '\n' | sed '/^[[:space:]]*$/d'
    return 0
  fi
  codex_fence_run_modes
}
