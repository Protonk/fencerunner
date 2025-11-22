#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="sysctl_hw_ncpu_read"
primary_capability_id="cap_sysctl_read_basic"
sysctl_key="${FENCE_SYSCTL_KEY:-hw.ncpu}"
printf -v command_executed "sysctl -n %q" "${sysctl_key}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
/usr/sbin/sysctl -n "${sysctl_key}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
value=$(printf '%s' "${stdout_text}" | head -n 1 | tr -d '\n')

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Read sysctl ${sysctl_key}"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied reading ${sysctl_key}"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted"
  elif [[ "${lower_err}" == *"unknown oid"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="Unknown sysctl key"
  else
    status="error"
    message="sysctl failed with exit code ${exit_code}"
  fi
fi

raw_value_flag=(--payload-raw-null "value")
if [[ -n "${value}" ]]; then
  raw_value_flag=(--payload-raw-field "value" "${value}")
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "sysctl" \
  --verb "read" \
  --target "${sysctl_key}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "sysctl_key" "${sysctl_key}" \
  "${raw_value_flag[@]}" \
  --operation-arg "sysctl_key" "${sysctl_key}"
