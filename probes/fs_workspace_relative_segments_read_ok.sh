#!/usr/bin/env bash
set -euo pipefail

# cap_fs_read_workspace_tree success case: read a workspace file using ./ and ../ segments.
emit_record_bin="${repo_root}/bin/emit-record"
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
portable_path_helper="${repo_root}/bin/portable-path"
if [[ ! -x "${portable_path_helper}" ]]; then
  echo "portable-path helper missing at ${portable_path_helper}. Build it with 'cargo build --release'." >&2
  exit 1
fi

portable_realpath() {
  "${portable_path_helper}" realpath "$1"
}

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_workspace_relative_segments_read_ok"
probe_version="1"
primary_capability_id="cap_fs_read_workspace_tree"
lines_to_read="${FENCE_FS_WORKSPACE_REL_SEGMENTS_LINES:-8}"
if ! [[ "${lines_to_read}" =~ ^[0-9]+$ ]]; then
  lines_to_read=8
fi
relative_target="${repo_root}/./docs/../README.md"
canonical_target=$(portable_realpath "${relative_target}")
if [[ -z "${canonical_target}" ]]; then
  canonical_target="${relative_target}"
fi
printf -v command_executed "head -n %q %q" "${lines_to_read}" "${relative_target}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
head -n "${lines_to_read}" "${relative_target}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Read ${lines_to_read} lines via relative workspace path"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied when reading workspace file"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted when reading workspace file"
  elif [[ "${lower_err}" == *"no such file"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="Workspace README missing"
  else
    status="error"
    errno_value=""
    message="Relative head read failed with exit code ${exit_code}"
  fi
fi

raw_json=$(jq -n \
  --arg relative_path "${relative_target}" \
  --arg canonical_path "${canonical_target}" \
  --argjson lines "${lines_to_read}" \
  --argjson stdout_length "${#stdout_text}" \
  --argjson stderr_length "${#stderr_text}" \
  '{relative_path: $relative_path,
    canonical_path: $canonical_path,
    via_relative_segments: true,
    lines_requested: $lines,
    stdout_length: $stdout_length,
    stderr_length: $stderr_length}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_json}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg read_mode "head" \
  --argjson lines "${lines_to_read}" \
  --arg via_relative_segments "true" \
  '{read_mode: $read_mode,
    lines: $lines,
    via_relative_segments: ($via_relative_segments == "true")}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "read" \
  --target "${relative_target}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
