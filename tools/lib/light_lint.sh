#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE' >&2
Usage: tools/lib/light_lint.sh <script> [<script> ...]

Runs a lightweight lint pass (bash -n) against the provided probe scripts.
The command exits non-zero if a script is missing or fails linting.
USAGE
}

if [[ $# -eq 0 ]]; then
  usage
  exit 1
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

done

exit ${status}
