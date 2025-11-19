#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Guard-rail summary:
#   * Loads the canonical capability catalog via capabilities_adapter.sh and
#     inspects probes/ to emit a normalized coverage map for documentation and
#     tooling.
#   * Hard-fails when probes omit required metadata or reference unknown
#     capability IDs so the resulting map never masks drift.
# -----------------------------------------------------------------------------
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
repo_root=$(cd "${script_dir}/.." >/dev/null 2>&1 && pwd)

capabilities_adapter="${repo_root}/tools/capabilities_adapter.sh"
if [[ ! -x "${capabilities_adapter}" ]]; then
  echo "generate_probe_coverage_map: missing adapter at ${capabilities_adapter}" >&2
  exit 1
fi

coverage_output="${1:-}" # default to stdout

capability_ids=()
while IFS= read -r capability_id; do
  capability_ids+=("${capability_id}")
done < <("${capabilities_adapter}" | jq -r 'keys[]')

if [[ ${#capability_ids[@]} -eq 0 ]]; then
  echo "generate_probe_coverage_map: adapter returned no capability IDs" >&2
  exit 1
fi

list_contains() {
  local needle="$1"
  shift || true
  for candidate in "$@"; do
    if [[ "${candidate}" == "${needle}" ]]; then
      return 0
    fi
  done
  return 1
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

status=0
probe_pairs=()

while IFS= read -r script; do
  probe_name=$(extract_var "${script}" "probe_name" || true)
  primary_cap=$(extract_var "${script}" "primary_capability_id" || true)

  if [[ -z "${probe_name}" ]]; then
    echo "generate_probe_coverage_map: ${script} is missing probe_name" >&2
    status=1
    continue
  fi
  if [[ -z "${primary_cap}" ]]; then
    echo "generate_probe_coverage_map: ${script} is missing primary_capability_id" >&2
    status=1
    continue
  fi
  if ! list_contains "${primary_cap}" "${capability_ids[@]}"; then
    echo "generate_probe_coverage_map: ${script} references unknown capability '${primary_cap}'" >&2
    status=1
    continue
  fi

  probe_pairs+=("${primary_cap}\t${probe_name}")

done < <(find "${repo_root}/probes" -type f -name '*.sh' -print | LC_ALL=C sort)

if [[ ${status} -ne 0 ]]; then
  exit ${status}
fi

base_map=$("${capabilities_adapter}" | jq -S 'with_entries({key: .key, value: {has_probe: false, probe_ids: []}})')

probe_json='[]'
if [[ ${#probe_pairs[@]} -gt 0 ]]; then
  probe_json=$(printf '%s\n' "${probe_pairs[@]}" | jq -R -s '
    split("\n")
    | map(select(length > 0))
    | map(split("\t"))
    | map({cap: .[0], probe: .[1]})
  ')
fi

coverage_json=$(printf '%s\n' "${base_map}" "${probe_json}" | jq -S -s '
  .[0] as $base |
  (.[1] // []) as $probes |
  reduce $probes[] as $p ($base;
    if has($p.cap) then
      .[$p.cap].has_probe = true |
      .[$p.cap].probe_ids |= ((. + [$p.probe]) | unique | sort)
    else
      .
    end
  )
')

if [[ -n "${coverage_output}" ]]; then
  printf '%s\n' "${coverage_json}" > "${coverage_output}"
  echo "generate_probe_coverage_map: wrote coverage to ${coverage_output}" >&2
else
  printf '%s\n' "${coverage_json}"
fi
