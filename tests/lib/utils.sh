#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${REPO_ROOT:-}" ]]; then
  REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." >/dev/null 2>&1 && pwd)
fi

extract_probe_var() {
  local file="$1"
  local var="$2"
  local line value trimmed first last
  line=$(grep -E "^[[:space:]]*${var}=" "$file" | head -n1 || true)
  if [[ -z "${line}" ]]; then
    return 1
  fi
  value=${line#*=}
  value=${value%%#*}
  value=$(printf '%s' "${value}" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')
  if [[ -n "${value}" ]]; then
    first=${value:0:1}
    last=${value: -1}
    if [[ "${first}" == '"' && "${last}" == '"' && ${#value} -ge 2 ]]; then
      value=${value:1:-1}
    elif [[ "${first}" == "'" && "${last}" == "'" && ${#value} -ge 2 ]]; then
      value=${value:1:-1}
    fi
  fi
  printf '%s\n' "${value}"
}
