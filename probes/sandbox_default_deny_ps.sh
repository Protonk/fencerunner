#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="sandbox_default_deny_ps"
primary_capability_id="cap_sandbox_default_deny"
ps_flags=("-p" "$$" "-o" "pid,comm")
printf -v command_executed "/bin/ps -p %d -o pid,comm" "$$"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
/bin/ps "${ps_flags[@]}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="ps listed current process"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Sandbox denied ps inspection"
  elif [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied running ps"
  else
    status="error"
    message="ps failed with exit code ${exit_code}"
  fi
fi

ps_flags_json=$(printf '%s\n' "${ps_flags[@]}" | python3 -c 'import sys, json; flags=[line.strip() for line in sys.stdin if line.strip()]; print(json.dumps(flags))')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "proc" \
  --verb "inspect" \
  --target "/bin/ps" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field-json "ps_flags" "${ps_flags_json}" \
  --operation-arg-json "ps_flags" "${ps_flags_json}"
