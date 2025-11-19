#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Keeps docs/data/probe_cap_coverage_map.json, the capability adapter, and
# probe metadata in sync. This prevents drift between human guidance and
# executable probes.
# -----------------------------------------------------------------------------
set -euo pipefail

# macOS still ships /bin/bash 3.2, so this script sticks to portable Bash
# features (no associative arrays, mapfile, etc.).

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)

source "${script_dir}/../library/utils.sh"

cd "${REPO_ROOT}"

status=0

echo "capability_map_sync: validating capability metadata"

capabilities_adapter="${REPO_ROOT}/tools/capabilities_adapter.sh"
if [[ ! -x "${capabilities_adapter}" ]]; then
  echo "capability_map_sync: missing adapter at ${capabilities_adapter}" >&2
  exit 1
fi

capability_ids=()
while IFS= read -r capability_id; do
  capability_ids+=("${capability_id}")
done < <("${capabilities_adapter}" | jq -r 'keys[]')
# The adapter reflects schema/capabilities.json, so its keys become the source of truth.

if [[ ${#capability_ids[@]} -eq 0 ]]; then
  echo "capability_map_sync: adapter returned no capability IDs" >&2
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

coverage_cap_ids=()
coverage_has_probe_flags=()
coverage_probe_lists=()

while IFS=$'\t' read -r cap_id has_probe probe_list; do
  coverage_cap_ids+=("${cap_id}")
  coverage_has_probe_flags+=("${has_probe}")
  coverage_probe_lists+=("${probe_list}")
  if [[ -z "${cap_id}" ]]; then
    echo "  [FAIL] capability_map_sync: empty capability id in coverage map" >&2
    status=1
    continue
  fi
  if [[ -z "${has_probe}" ]]; then
    echo "  [FAIL] capability_map_sync: missing has_probe flag for capability '${cap_id}'" >&2
    status=1
  fi
  if ! list_contains "${cap_id}" "${capability_ids[@]}"; then
    echo "  [FAIL] capability_map_sync: coverage references unknown capability '${cap_id}'" >&2
    status=1
  fi

done < <(jq -r 'to_entries[] | [.key, (.value.has_probe|tostring), (.value.probe_ids|join(","))] | @tsv' docs/data/probe_cap_coverage_map.json)
# Each coverage entry includes whether a probe exists plus the explicit list for readability.

for cap in "${capability_ids[@]}"; do
  if ! list_contains "${cap}" "${coverage_cap_ids[@]}"; then
    echo "  [FAIL] capability_map_sync: docs/data/probe_cap_coverage_map.json missing entry for '${cap}'" >&2
    status=1
  fi

done

probe_scripts=()
while IFS= read -r script; do
  probe_scripts+=("${script}")
done < <(find "${REPO_ROOT}/probes" -type f -name '*.sh' -print | LC_ALL=C sort)
if [[ ${#probe_scripts[@]} -eq 0 ]]; then
  echo "capability_map_sync: no probes found" >&2
  exit 1
fi

probe_names=()
probe_caps=()

for script in "${probe_scripts[@]}"; do
  probe_name=$(extract_probe_var "${script}" "probe_name" || true)
  primary_cap=$(extract_probe_var "${script}" "primary_capability_id" || true)

  if [[ -z "${probe_name}" ]]; then
    echo "  [FAIL] capability_map_sync: ${script} is missing probe_name" >&2
    status=1
    continue
  fi

  if [[ -z "${primary_cap}" ]]; then
    echo "  [FAIL] capability_map_sync: ${script} is missing primary_capability_id" >&2
    status=1
    continue
  fi

  probe_names+=("${probe_name}")
  probe_caps+=("${primary_cap}")

  if ! list_contains "${primary_cap}" "${capability_ids[@]}"; then
    echo "  [FAIL] capability_map_sync: ${script} references unknown capability '${primary_cap}'" >&2
    status=1
  fi

done

probe_cap_for_name() {
  local target="$1"
  for idx in "${!probe_names[@]}"; do
    if [[ "${probe_names[$idx]}" == "${target}" ]]; then
      printf '%s\n' "${probe_caps[$idx]}"
      return 0
    fi
  done
  return 1
}

for idx in "${!coverage_cap_ids[@]}"; do
  cap_id="${coverage_cap_ids[$idx]}"
  has_probe_flag="${coverage_has_probe_flags[$idx]}"
  coverage_value="${coverage_probe_lists[$idx]}"

  coverage_array=()
  if [[ -n "${coverage_value}" ]]; then
    IFS=',' read -r -a coverage_array <<< "${coverage_value}"
  fi
  coverage_array_len=${#coverage_array[@]}

  actual_array=()
  for probe_idx in "${!probe_names[@]}"; do
    if [[ "${probe_caps[$probe_idx]}" == "${cap_id}" ]]; then
      actual_array+=("${probe_names[$probe_idx]}")
    fi
  done
  actual_array_len=${#actual_array[@]}
  # coverage_array -> docs, actual_array -> live probes.

  if [[ "${has_probe_flag}" == "true" && ${actual_array_len} -eq 0 ]]; then
    echo "  [FAIL] capability_map_sync: ${cap_id} marked has_probe=true but no probes declare it" >&2
    status=1
  fi

  if [[ "${has_probe_flag}" == "false" && ${actual_array_len} -gt 0 ]]; then
    echo "  [FAIL] capability_map_sync: ${cap_id} marked has_probe=false but probes ${actual_array[*]} target it" >&2
    status=1
  fi

  if (( coverage_array_len > 0 )); then
    # When the docs list explicit probes ensure each one still exists and points back.
    for listed_probe in "${coverage_array[@]}"; do
      if [[ -z "${listed_probe}" ]]; then
        continue
      fi
      if ! list_contains "${listed_probe}" "${probe_names[@]}"; then
        echo "  [FAIL] capability_map_sync: ${cap_id} lists unknown probe '${listed_probe}'" >&2
        status=1
        continue
      fi
      if ! listed_cap=$(probe_cap_for_name "${listed_probe}"); then
        echo "  [FAIL] capability_map_sync: unable to resolve capability for probe '${listed_probe}'" >&2
        status=1
        continue
      fi
      if [[ "${listed_cap}" != "${cap_id}" ]]; then
        echo "  [FAIL] capability_map_sync: ${listed_probe} in coverage for ${cap_id} but script targets ${listed_cap}" >&2
        status=1
      fi
    done
  fi

  if (( actual_array_len > 0 )); then
    # Conversely, every real probe should be described in coverage docs.
    for actual_probe in "${actual_array[@]}"; do
      if (( coverage_array_len > 0 )) && list_contains "${actual_probe}" "${coverage_array[@]}"; then
        continue
      fi
      echo "  [FAIL] capability_map_sync: ${cap_id} missing probe '${actual_probe}' in coverage list" >&2
      status=1
    done
  fi

done

if [[ ${status} -ne 0 ]]; then
  exit ${status}
fi

echo "capability_map_sync: PASS"
