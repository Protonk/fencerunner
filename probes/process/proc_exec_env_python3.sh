#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="proc_exec_env_python3"
probe_version="1"
primary_capability_id="cap_proc_exec_toolchain_outside_workspace"

env_bin="${FENCE_PROC_ENV_BIN:-/usr/bin/env}"
python_code=$(cat <<'PY'
import json
import platform
import sys

info = {
    "sys_executable": sys.executable,
    "sys_version": sys.version,
    "platform": platform.platform(),
    "argv": sys.argv,
}
print(json.dumps(info))
PY
)

printf -v command_executed "%q %q %q %q" "${env_bin}" "python3" "-c" "${python_code}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
"${env_bin}" python3 -c "${python_code}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

raw_json='{}'
if [[ -s "${stdout_tmp}" ]] && jq -e . "${stdout_tmp}" >/dev/null 2>&1; then
  raw_json=$(jq -c '.' "${stdout_tmp}")
fi

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="/usr/bin/env python3 succeeded"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied executing /usr/bin/env python3"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted executing /usr/bin/env python3"
  elif [[ "${lower_err}" == *"no such file"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="python3 not found via env"
  else
    status="error"
    message="/usr/bin/env python3 failed with exit code ${exit_code}"
  fi
fi

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_json}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg env_bin "${env_bin}" \
  --arg python_code "${python_code}" \
  --argjson python_info "${raw_json}" \
  '{argv: [$env_bin, "python3", "-c", $python_code],
    interpreter_reported: (if $python_info == {} then null else $python_info.sys_executable end),
    reported_version: (if $python_info == {} then null else $python_info.sys_version end)}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "proc" \
  --verb "exec_env_python3" \
  --target "${env_bin}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
