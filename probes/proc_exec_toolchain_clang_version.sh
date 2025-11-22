#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="proc_exec_toolchain_clang_version"
primary_capability_id="cap_proc_exec_toolchain_outside_workspace"
default_clang_bin="${FENCE_CLANG_BIN:-/usr/bin/clang}"
clang_bin="${default_clang_bin}"
if [[ ! -x "${clang_bin}" ]]; then
  if command -v clang >/dev/null 2>&1; then
    clang_bin=$(command -v clang)
  fi
fi
printf -v command_executed "%q --version" "${clang_bin}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
"${clang_bin}" --version >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="clang executed successfully"
elif [[ ${exit_code} -eq 127 ]] || [[ "${lower_err}" == *"command not found"* ]]; then
  status="error"
  errno_value="ENOENT"
  message="clang binary missing"
elif [[ ${exit_code} -eq 126 ]] || [[ "${lower_err}" == *"permission denied"* ]]; then
  status="denied"
  errno_value="EACCES"
  message="Sandbox denied executing clang"
elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
  status="denied"
  errno_value="EPERM"
  message="Operation not permitted executing clang"
else
  status="error"
  message="clang exited with ${exit_code}"
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "proc" \
  --verb "exec" \
  --target "${clang_bin}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "clang_bin" "${clang_bin}" \
  --payload-raw-field "default_clang_bin" "${default_clang_bin}" \
  --payload-raw-field "stdout" "${stdout_text}" \
  --payload-raw-field "stderr" "${stderr_text}" \
  --payload-raw-field-json "stdout_length" "${#stdout_text}" \
  --operation-arg "binary" "${clang_bin}" \
  --operation-arg-json "args" "[\"--version\"]"
