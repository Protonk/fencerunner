#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
cd "${repo_root}"

suites=(
  "static_probe_contract"
  "capability_map_sync"
  "boundary_object_schema"
  "harness_smoke"
)

status=0
for suite in "${suites[@]}"; do
  script="tests/${suite}.sh"
  if [[ ! -x "${script}" ]]; then
    echo "Missing test script: ${script}" >&2
    status=1
    continue
  fi
  echo "Running ${suite}..."
  if "${script}"; then
    echo "${suite}: PASS"
  else
    echo "${suite}: FAIL"
    status=1
  fi
  echo
done

exit ${status}
