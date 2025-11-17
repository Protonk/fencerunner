#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE' >&2
Usage: tools/lib/light_lint.sh <script> [<script> ...]

Runs a lightweight lint pass (bash -n and shellcheck when available) against
the provided probe scripts. The command exits non-zero if a script is missing
or fails linting.
USAGE
}

if [[ $# -eq 0 ]]; then
  usage
  exit 1
fi

shellcheck_bin=$(command -v shellcheck 2>/dev/null || true)
shellcheck_available=1
if [[ -z "${shellcheck_bin}" ]]; then
  shellcheck_available=0
  echo "light_lint: shellcheck not found; skipping shellcheck stage" >&2
fi

status=0
for script in "$@"; do
  if [[ ! -f "${script}" ]]; then
    echo "light_lint: missing script '${script}'" >&2
    status=1
    continue
  fi

  if ! bash -n "${script}" >/dev/null 2>&1; then
    echo "light_lint: bash -n failed for '${script}'" >&2
    status=1
    continue
  fi

  if [[ ${shellcheck_available} -eq 1 ]]; then
    if ! "${shellcheck_bin}" "${script}"; then
      echo "light_lint: shellcheck reported issues for '${script}'" >&2
      status=1
    fi
  fi
done

exit ${status}
