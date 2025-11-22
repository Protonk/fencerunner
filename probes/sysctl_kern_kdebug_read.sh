#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="sysctl_kern_kdebug_read"
probe_version="1"
primary_capability_id="cap_sysctl_read_sensitive"

sysctl_bin="${FENCE_SYSCTL_BIN:-/usr/sbin/sysctl}"
target_key="${FENCE_SYSCTL_KDEBUG_KEY:-kern.kdebug}"

printf -v command_executed "%q -n %q" "${sysctl_bin}" "${target_key}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}"
}
trap cleanup EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

if [[ ! -x "${sysctl_bin}" ]]; then
  status="error"
  errno_value="ENOENT"
  message="sysctl binary not found or not executable"
  raw_exit_code="1"
  stdout_text=""
  stderr_text="sysctl bin ${sysctl_bin} missing"
else
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
    message="${target_key} unavailable on this system"
  else
    status="error"
    message="sysctl ${target_key} failed with exit code ${exit_code}"
  fi
fi

raw_stdout_flag=(--payload-raw-null "stdout")
if [[ -n "${stdout_text:-}" ]]; then
  raw_stdout_flag=(--payload-raw-field "stdout" "${stdout_text}")
fi
raw_stderr_flag=(--payload-raw-null "stderr")
if [[ -n "${stderr_text:-}" ]]; then
  raw_stderr_flag=(--payload-raw-field "stderr" "${stderr_text}")
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "sysctl" \
  --verb "read" \
  --target "${target_key}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text:-}" \
  --payload-stderr "${stderr_text:-}" \
  --payload-raw-field "key" "${target_key}" \
  --payload-raw-field "sysctl_bin" "${sysctl_bin}" \
  --payload-raw-field-json "exit_code" "${raw_exit_code:-0}" \
  "${raw_stdout_flag[@]}" \
  "${raw_stderr_flag[@]}" \
  --operation-arg "key" "${target_key}" \
  --operation-arg "sysctl_bin" "${sysctl_bin}"
