#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_read_system_version_plist"
primary_capability_id="cap_fs_read_system_roots"
target_path="${FENCE_SYSTEM_VERSION_PLIST:-/System/Library/CoreServices/SystemVersion.plist}"
lines_to_read="${FENCE_SYSTEM_VERSION_LINES:-5}"
if ! [[ "${lines_to_read}" =~ ^[0-9]+$ ]]; then
  lines_to_read=5
fi
printf -v command_executed "head -n %q %q" "${lines_to_read}" "${target_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
head -n "${lines_to_read}" "${target_path}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Read ${lines_to_read} lines from ${target_path}"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied reading ${target_path}"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted reading ${target_path}"
  elif [[ "${lower_err}" == *"no such file"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="SystemVersion.plist missing"
  else
    status="error"
    errno_value=""
    message="Read failed with exit code ${exit_code}"
  fi
fi

if [[ -e "${target_path}" ]]; then
  target_exists_json="true"
else
  target_exists_json="false"
fi

raw_json=$(jq -n \
  --argjson lines "${lines_to_read}" \
  --argjson stdout_length "${#stdout_text}" \
  --argjson stderr_length "${#stderr_text}" \
  --argjson target_exists "${target_exists_json}" \
  '{target_exists: $target_exists, lines_requested: $lines, stdout_length: $stdout_length, stderr_length: $stderr_length}')

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
  '{read_mode: $read_mode, lines: $lines}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "read" \
  --target "${target_path}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
