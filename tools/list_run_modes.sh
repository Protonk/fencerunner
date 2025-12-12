#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Shared run-mode definitions for bash helpers.
#
# Goals:
# - Keep contract gate mode coverage aligned with the modes the harness cares
#   about (baseline).
# - Allow overrides via PROBE_CONTRACT_MODES for tests or rapid experiments.
# -----------------------------------------------------------------------------
set -euo pipefail

contract_gate_modes() {
  local override="${PROBE_CONTRACT_MODES:-}"
  if [[ -n "${override}" ]]; then
    printf '%s' "${override}" | tr ',' ' ' | tr -s ' ' '\n' | sed '/^[[:space:]]*$/d'
    return 0
  fi
  printf '%s\n' "baseline"
}
