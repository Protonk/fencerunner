#!/usr/bin/env bash
set -euo pipefail

# Variant for cap_fs_read_system_roots: list metadata for /System/Library (or override) to ensure directory reads are allowed.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_read_system_library_ls"
primary_capability_id="cap_fs_read_system_roots"
target_dir="${FENCE_SYSTEM_ROOT_DIR:-/System/Library}"
printf -v command_executed "ls -ld %q" "${target_dir}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
ls -ld "${target_dir}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Listed ${target_dir} metadata"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied listing ${target_dir}"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted listing ${target_dir}"
  elif [[ "${lower_err}" == *"no such file or directory"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="${target_dir} missing"
  else
    status="error"
    errno_value=""
    message="ls failed with exit code ${exit_code}"
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
  --target "${target_dir}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "target" "${target_dir}" \
  --payload-raw-field "stdout" "${stdout_text}" \
  --payload-raw-field "stderr" "${stderr_text}" \
  --payload-raw-field-json "exit_code" "${exit_code}" \
  --operation-arg "path_type" "directory" \
  --operation-arg "read_mode" "metadata"
