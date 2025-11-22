#!/usr/bin/env bash
set -euo pipefail

# Variant for cap_sysctl_read_sensitive: read the security.mac.proc_enforce key to observe policy on MAC enforcement toggles.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="sysctl_security_mac_proc_enforce"
primary_capability_id="cap_sysctl_read_sensitive"
sysctl_bin="${FENCE_SYSCTL_BIN:-/usr/sbin/sysctl}"
target_key="${FENCE_SYSCTL_SECURITY_KEY:-security.mac.proc_enforce}"

printf -v command_executed "%q -n %q" "${sysctl_bin}" "${target_key}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
"${sysctl_bin}" -n "${target_key}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Read ${target_key}"
elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
  status="denied"
  errno_value="EPERM"
  message="sysctl ${target_key} blocked: operation not permitted"
elif [[ "${lower_err}" == *"permission denied"* ]]; then
  status="denied"
  errno_value="EACCES"
  message="sysctl ${target_key} blocked: permission denied"
elif [[ "${lower_err}" == *"unknown oid"* ]] || [[ "${lower_err}" == *"no such file or directory"* ]]; then
  status="partial"
  errno_value="ENOENT"
  message="${target_key} unavailable"
else
  status="error"
  errno_value=""
  message="sysctl ${target_key} failed with exit code ${exit_code}"
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "sysctl" \
  --verb "read" \
  --target "${target_key}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "key" "${target_key}" \
  --payload-raw-field "sysctl_bin" "${sysctl_bin}" \
  --payload-raw-field "stdout" "${stdout_text}" \
  --payload-raw-field "stderr" "${stderr_text}" \
  --payload-raw-field-json "exit_code" "${exit_code}" \
  --operation-arg "key" "${target_key}" \
  --operation-arg "sysctl_bin" "${sysctl_bin}"
