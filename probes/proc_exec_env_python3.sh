#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
json_extract_bin="${repo_root}/bin/json-extract"

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
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

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
python_executable_json="null"
python_version_json="null"
if [[ -x "${json_extract_bin}" && -s "${stdout_tmp}" ]]; then
  if parsed=$("${json_extract_bin}" --file "${stdout_tmp}" --type object --default "{}" 2>/dev/null); then
    raw_json="${parsed}"
    python_executable_json=$("${json_extract_bin}" --file "${stdout_tmp}" --pointer "/sys_executable" --type string --default "null" 2>/dev/null || printf 'null')
    python_version_json=$("${json_extract_bin}" --file "${stdout_tmp}" --pointer "/sys_version" --type string --default "null" 2>/dev/null || printf 'null')
  fi
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
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw "${raw_json}" \
  --operation-arg "argv" "${env_bin} python3 -c <inline>" \
  --operation-arg "env_bin" "${env_bin}" \
  --operation-arg "python_code" "${python_code}" \
  --operation-arg-json "interpreter_reported" "${python_executable_json}" \
  --operation-arg-json "reported_version" "${python_version_json}"
