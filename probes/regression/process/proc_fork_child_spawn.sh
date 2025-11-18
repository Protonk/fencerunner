#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="proc_fork_child_spawn"
primary_capability_id="cap_proc_fork_and_child_spawn"

fork_python_script=$(mktemp)
cat <<'PY' >"${fork_python_script}"
import os
import sys

child_pid = os.fork()
if child_pid == 0:
    sys.stdout.write(f"child pid={os.getpid()} parent={os.getppid()}\n")
    sys.stdout.flush()
    os._exit(0)
else:
    waited_pid, status = os.waitpid(child_pid, 0)
    sys.stdout.write(f"parent waited pid={waited_pid} status={status}\n")
    sys.stdout.flush()
PY

printf -v command_executed "python3 %q" "${fork_python_script}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}" "${fork_python_script}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
python3 "${fork_python_script}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Forked and reaped child process"
elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
  status="denied"
  errno_value="EPERM"
  message="Fork denied: operation not permitted"
elif [[ "${lower_err}" == *"resource temporarily unavailable"* ]]; then
  status="denied"
  errno_value="EAGAIN"
  message="Fork denied: resource temporarily unavailable"
else
  status="error"
  errno_value=""
  message="Fork test failed with exit code ${exit_code}"
fi

child_observed="false"
if [[ "${stdout_text}" == *"child pid="* ]]; then
  child_observed="true"
fi

raw_payload=$(jq -n \
  --arg stdout "${stdout_text}" \
  --arg stderr "${stderr_text}" \
  --argjson child_observed "$child_observed" \
  '{child_observed: $child_observed == true, stdout_lines: ($stdout | split("\n") | map(select(length > 0))), stderr_lines: ($stderr | split("\n") | map(select(length > 0)))}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg attempt "python3 os.fork + waitpid" \
  '{attempt: $attempt}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "proc" \
  --verb "fork" \
  --target "python3 os.fork" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
