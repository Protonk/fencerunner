#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="agent_sandbox_env_marker"
primary_capability_id="cap_agent_sandbox_env_marker"
marker_var="CODEX_SANDBOX_ENV_VAR"
printf -v command_executed "printenv %s" "${marker_var}"

tmp_stage_dir="${repo_root}/tmp/${probe_name}"
mkdir -p "${tmp_stage_dir}" 2>/dev/null || true

detect_errno_from_text() {
  local text="$1"
  if [[ "${text}" == *"Operation not permitted"* ]]; then
    printf 'EPERM'
  elif [[ "${text}" == *"Permission denied"* ]]; then
    printf 'EACCES'
  else
    printf ''
  fi
}

stdout_text=""
stderr_text=""
set +e
command_output=$(printenv "${marker_var}" 2>&1)
exit_code=$?
set -e

if [[ ${exit_code} -eq 0 ]]; then
  stdout_text="${command_output}"
else
  stderr_text="${command_output}"
fi

raw_exit_code="${exit_code}"
marker_value=$(printf '%s' "${stdout_text}" | tr -d '\n')
fallback_marker="${CODEX_SANDBOX:-}"

status="partial"
errno_value=""
message="Sandbox env marker absent"

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="${marker_var}=${marker_value}"
elif [[ ${exit_code} -eq 1 ]]; then
  status="partial"
  if [[ -n "${fallback_marker}" ]]; then
    message="${marker_var} unset; CODEX_SANDBOX=${fallback_marker} present"
  else
    message="Sandbox env marker absent"
  fi
else
  errno_value=$(detect_errno_from_text "${stderr_text}")
  if [[ -n "${errno_value}" ]]; then
    status="denied"
  else
    status="error"
  fi
  message="printenv exited with ${exit_code}"
fi

emit_args=(
  --run-mode "${run_mode}"
  --probe-name "${probe_name}"
  --probe-version "1"
  --primary-capability-id "${primary_capability_id}"
  --command "${command_executed}"
  --category "agent_policy"
  --verb "inspect"
  --target "${marker_var}"
  --status "${status}"
  --errno "${errno_value}"
  --message "${message}"
  --raw-exit-code "${raw_exit_code}"
  --operation-arg "marker_var" "${marker_var}"
  --payload-stdout "${stdout_text}"
  --payload-stderr "${stderr_text}"
  --payload-raw-field "marker_var" "${marker_var}"
  --payload-raw-field "stdout" "${stdout_text}"
  --payload-raw-field "stderr" "${stderr_text}"
)

if [[ -n "${marker_value}" ]]; then
  emit_args+=(--payload-raw-field "marker_value" "${marker_value}")
else
  emit_args+=(--payload-raw-null "marker_value")
fi

if [[ -n "${fallback_marker}" ]]; then
  emit_args+=(--payload-raw-field "fallback_marker" "${fallback_marker}")
else
  emit_args+=(--payload-raw-null "fallback_marker")
fi

"${emit_record_bin}" "${emit_args[@]}"
