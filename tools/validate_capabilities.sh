#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
repo_root=$(cd "${script_dir}/.." >/dev/null 2>&1 && pwd)

capabilities_adapter="${repo_root}/tools/capabilities_adapter.sh"
if [[ ! -x "${capabilities_adapter}" ]]; then
  echo "validate_capabilities: missing adapter at ${capabilities_adapter}" >&2
  exit 1
fi

known_capability_ids=()
while IFS= read -r capability_id; do
  if [[ -n "${capability_id}" ]]; then
    known_capability_ids+=("${capability_id}")
  fi
done < <("${capabilities_adapter}" | jq -r 'keys[]')

if [[ ${#known_capability_ids[@]} -eq 0 ]]; then
  echo "validate_capabilities: adapter returned no capability IDs" >&2
  exit 1
fi

status=0

check_capability() {
  local candidate="$1"
  local context="$2"
  local found=1
  for cap in "${known_capability_ids[@]}"; do
    if [[ "${cap}" == "${candidate}" ]]; then
      found=0
      break
    fi
  done
  if [[ ${found} -ne 0 ]]; then
    echo "validate_capabilities: ${context} references unknown capability '${candidate}'" >&2
    status=1
  fi
}

extract_var() {
  local file="$1"
  local var="$2"
  local line value first last value_length
  line=$(grep -E "^[[:space:]]*${var}=" "${file}" | head -n1 || true)
  if [[ -z "${line}" ]]; then
    return 1
  fi
  value=${line#*=}
  value=${value%%#*}
  value=$(printf '%s' "${value}" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')
  if [[ -n "${value}" ]]; then
    first=${value:0:1}
    last=${value: -1}
    value_length=${#value}
    if [[ "${first}" == '"' && "${last}" == '"' && ${value_length} -ge 2 ]]; then
      value=${value:1:value_length-2}
    elif [[ "${first}" == "'" && "${last}" == "'" && ${value_length} -ge 2 ]]; then
      value=${value:1:value_length-2}
    fi
  fi
  printf '%s\n' "${value}"
}

shopt -s nullglob
probe_files=("${repo_root}/probes/"*.sh "${repo_root}/tests/library/fixtures/"*.sh)
shopt -u nullglob

for script in "${probe_files[@]}"; do
  if [[ ! -f "${script}" ]]; then
    continue
  fi

  primary_cap=$(extract_var "${script}" "primary_capability_id" || true)
  if [[ -n "${primary_cap}" ]]; then
    check_capability "${primary_cap}" "${script} primary_capability_id"
  fi

  secondary_cap=$(extract_var "${script}" "secondary_capability_id" || true)
  if [[ -n "${secondary_cap}" ]]; then
    check_capability "${secondary_cap}" "${script} secondary_capability_id"
  fi
done

json_dirs=("${repo_root}/out")
for dir in "${json_dirs[@]}"; do
  if [[ ! -d "${dir}" ]]; then
    continue
  fi
  while IFS= read -r -d '' json_file; do
    while IFS= read -r json_cap; do
      if [[ -z "${json_cap}" ]]; then
        continue
      fi
      check_capability "${json_cap}" "${json_file}"
    done < <(jq -r '
      ( .capability_context? // {} ) as $ctx |
      [
        .probe.primary_capability_id,
        (.probe.secondary_capability_ids // [])[],
        ($ctx.primary // {} | .id // null),
        (($ctx.secondary // [])[]? | .id)
      ]
      | map(select(. != null and . != ""))
      | .[]
    ' "${json_file}")
  done < <(find "${dir}" -type f -name '*.json' -print0)
done

if [[ ${status} -ne 0 ]]; then
  echo "validate_capabilities: FAIL" >&2
  exit ${status}
fi

echo "validate_capabilities: PASS"
