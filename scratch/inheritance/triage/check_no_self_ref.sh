#!/usr/bin/env bash
set -euo pipefail

root="${1:-scratch/inheritance}"
if [[ ! -d "${root}" ]]; then
  echo "usage: scratch/inheritance/triage/check_no_self_ref.sh [ROOT_DIR]" >&2
  echo "error: not a directory: ${root}" >&2
  exit 2
fi

failed=0
for run_dir in "${root}"/*; do
  [[ -d "${run_dir}" ]] || continue
  for legacy_path in "${run_dir}"/*.legacy; do
    [[ -f "${legacy_path}" ]] || continue
    script_id="$(basename "${legacy_path}" .legacy)"

    if rg -n "^[[:space:]]*[^#].*\\b${script_id}\\.sh\\b" "${legacy_path}" >/dev/null 2>&1; then
      echo "fail: ${legacy_path} references ${script_id}.sh" >&2
      rg -n "^[[:space:]]*[^#].*\\b${script_id}\\.sh\\b" "${legacy_path}" >&2 || true
      failed=1
    fi
  done
done

if [[ "${failed}" -ne 0 ]]; then
  exit 1
fi

