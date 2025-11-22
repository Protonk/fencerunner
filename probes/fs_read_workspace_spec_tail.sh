#!/usr/bin/env bash
set -euo pipefail

# Variant for cap_fs_read_workspace_tree: read the tail of a nested workspace file to show deeper path coverage.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_read_workspace_spec_tail"
primary_capability_id="cap_fs_read_workspace_tree"
target_path="${repo_root}/schema/capabilities.json"
lines_to_read="${FENCE_FS_READ_WORKSPACE_SPEC_LINES:-10}"
if ! [[ "${lines_to_read}" =~ ^[0-9]+$ ]]; then
  lines_to_read=10
fi
printf -v command_executed "tail -n %q %q" "${lines_to_read}" "${target_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

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
  message="Tailed ${lines_to_read} lines from capability catalog"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied reading capability catalog"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted reading capability catalog"
  elif [[ "${lower_err}" == *"no such file"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="Capability catalog file missing"
  else
    status="error"
    errno_value=""
    message="Tail failed with exit code ${exit_code}"
  fi
fi

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
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "target_path" "${target_path}" \
  --payload-raw-field-json "lines_requested" "${lines_to_read}" \
  --payload-raw-field-json "stdout_length" "${#stdout_text}" \
  --payload-raw-field-json "stderr_length" "${#stderr_text}" \
  --operation-arg "read_mode" "tail" \
  --operation-arg-json "lines" "${lines_to_read}"
