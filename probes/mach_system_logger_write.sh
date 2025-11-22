#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="mach_system_logger_write"
primary_capability_id="cap_mach_lookup_system_logger"
log_message="codex-fence mach logger $(date -u +%Y-%m-%dT%H:%M:%SZ) $$"
log_facility="user.notice"
printf -v command_executed "logger -s -p %q -- %q" "${log_facility}" "${log_message}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
/usr/bin/logger -s -p "${log_facility}" -- "${log_message}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Logger message sent"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Logger denied"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted"
  else
    status="error"
    message="logger exited with ${exit_code}"
  fi
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "ipc" \
  --verb "mach-lookup" \
  --target "com.apple.system.logger" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "message" "${log_message}" \
  --payload-raw-field "facility" "${log_facility}" \
  --operation-arg "facility" "${log_facility}" \
  --operation-arg "message" "${log_message}"
