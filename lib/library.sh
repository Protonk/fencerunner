#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# lib/library.sh
#
# Runner-owned probe library (Bash 3.2 compatible).
#
# This file is part of the probe contract. It is intentionally plain Bash and
# heavily commented so probe authors can audit behavior without spelunking Rust.
#
# What this library provides:
# - `commit_help_me <ensure|detect|emit> <commitment.id>`
#     Record a commitment enrollment for the current probe run.
#     This does not validate against `<RUN_DIR>/commitments.json`; enrollments
#     are treated as a trustworthy signal from a willing author, not an enforced
#     gate.
#
#     Under the hood, this delegates to the runner-provided helper `commit-help-me`
#     (expected to be on PATH), which appends `id|help` lines to a
#     runner-provided file at:
#       FENCERUNNER_COMMITMENT_ENROLLMENTS_PATH
#
#     The `emit-record` helper reads that same file and serializes the
#     enrollments into `/context/commitments` in the emitted boundary object.
#
# Required probe bootstrap (probe contract):
#   source "${FENCERUNNER_ROOT}/lib/library.sh"
# -----------------------------------------------------------------------------

# Probes are expected to enable strict mode themselves (`set -euo pipefail`).
# We avoid toggling `-e`/`-u` here because this file is sourced, but we do set
# pipefail as a minimal safety baseline.
set -o pipefail

# Sourcing is allowed to be idempotent. If a probe (or a test harness) sources
# this file twice, do nothing on the second load so we don't reset enrollments
# or helper paths.
if [[ -n "${FENCERUNNER_LIBRARY_LOADED:-}" ]]; then
  return 0
fi
FENCERUNNER_LIBRARY_LOADED=1

# Probe execution must define where the run dir lives. fencerunner sets this to
# the selected run directory (flat directory containing the triad + probes).
if [[ -z "${FENCERUNNER_RUN_DIR:-}" ]]; then
  echo "library.sh: FENCERUNNER_RUN_DIR is not set (run probes via fencerunner)" >&2
  return 1
fi

fencerunner__resolve_commit_help_me() {
  local on_path=""

  # First preference: whatever fencerunner put on PATH (runner-owned shims).
  on_path="$(command -v commit-help-me 2>/dev/null || true)"
  if [[ -n "${on_path}" && -x "${on_path}" ]]; then
    printf '%s\n' "${on_path}"
    return 0
  fi

  # Fallback: when running under fencerunner, `FENCERUNNER_ROOT` points at a
  # runner-owned directory that contains `bin/commit-help-me`.
  if [[ -n "${FENCERUNNER_ROOT:-}" ]]; then
    local candidate=""
    candidate="${FENCERUNNER_ROOT}/bin/commit-help-me"
    if [[ -x "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  fi

  return 1
}

# Resolve the commit-help-me helper used for enrollment tracking. We
# intentionally use the hyphenated helper name to avoid confusion with the
# `commit_help_me` Bash function.
if [[ -z "${commit_help_me_bin:-}" ]]; then
  commit_help_me_bin="$(fencerunner__resolve_commit_help_me)" || {
    echo "library.sh: unable to locate commit-help-me (run probes via fencerunner)" >&2
    return 1
  }
fi

# Enroll in a commitment (record-only).
#
# The compiled helper returns 1 for contract violations like duplicate pairs or
# invalid ids/verbs; probes should treat any non-zero return as a hard error.
commit_help_me() {
  local help="${1:-}"
  local commitment_id="${2:-}"
  if [[ $# -ne 2 || -z "${help}" || -z "${commitment_id}" ]]; then
    echo "commit_help_me: usage: commit_help_me <ensure|detect|emit> <commitment-id>" >&2
    return 1
  fi

  "${commit_help_me_bin}" "${help}" "${commitment_id}" >/dev/null
}
