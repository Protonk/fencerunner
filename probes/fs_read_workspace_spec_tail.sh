#!/usr/bin/env bash
set -euo pipefail

# Variant for cap_fs_read_workspace_tree: read the tail of a nested workspace file to show deeper path coverage.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_read_workspace_spec_tail"
primary_capability_id="cap_fs_read_workspace_tree"
target_path="${repo_root}/spec/capabilities.yaml"
lines_to_read="${FENCE_FS_READ_WORKSPACE_SPEC_LINES:-10}"
if ! [[ "${lines_to_read}" =~ ^[0-9]+$ ]]; then
  lines_to_read=10
fi
printf -v command_executed "tail -n %q %q" "${lines_to_read}" "${target_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
tail -n "${lines_to_read}" "${target_path}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Tailed ${lines_to_read} lines from workspace spec"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied reading workspace spec"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted reading workspace spec"
  elif [[ "${lower_err}" == *"no such file"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="Workspace spec file missing"
  else
    status="error"
    errno_value=""
    message="Tail failed with exit code ${exit_code}"
  fi
fi

raw_json=$(jq -n \
  --arg target_path "${target_path}" \
  --argjson lines "${lines_to_read}" \
  --argjson stdout_length "${#stdout_text}" \
  --argjson stderr_length "${#stderr_text}" \
  '{target_path: $target_path, lines_requested: $lines, stdout_length: $stdout_length, stderr_length: $stderr_length}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_json}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg read_mode "tail" \
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
