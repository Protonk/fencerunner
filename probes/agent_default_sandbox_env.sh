#!/usr/bin/env bash
set -euo pipefail

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="agent_default_sandbox_env"
primary_capability_id="cap_agent_default_sandboxing"

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

candidate_vars=(
  "CODEX_SANDBOX"
  "CODEX_SANDBOX_ENV_VAR"
)

regex='^(CODEX_SANDBOX|CODEX_SANDBOX_ENV_VAR)='
printf -v command_executed "env | grep -m 1 -E %q" "${regex}"

status="partial"
errno_value=""
message="No sandbox env marker detected"
raw_exit_code=""
stdout_text=""
stderr_text=""

set +e
env_output=$(env 2>&1)
env_exit_code=$?
set -e

detected_var=""
detected_value=""

if [[ ${env_exit_code} -ne 0 ]]; then
  raw_exit_code="${env_exit_code}"
  stderr_text="${env_output}"
  stdout_text=""
  status="error"
  errno_value=""
  if [[ "${stderr_text}" == *"Operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
  elif [[ "${stderr_text}" == *"Permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
  fi
  message="env command failed with exit code ${env_exit_code}"
else
  set +e
  stdout_text=$(printf '%s\n' "${env_output}" | grep -m 1 -E "${regex}")
  grep_exit_code=$?
  set -e
  raw_exit_code="${grep_exit_code}"

  if [[ ${grep_exit_code} -eq 0 ]]; then
    first_line="${stdout_text%%$'\n'*}"
    detected_var="${first_line%%=*}"
    detected_value="${first_line#*=}"
    status="success"
    message="Sandbox env marker ${detected_var}=${detected_value}"
  elif [[ ${grep_exit_code} -eq 1 ]]; then
    status="partial"
    message="Sandbox env marker absent"
  else
    status="error"
    message="env scan for sandbox marker failed with exit code ${grep_exit_code}"
  fi
fi

emit_cmd=(
  "${emit_record_bin}"
  --run-mode "${run_mode}"
  --probe-name "${probe_name}"
  --probe-version "1"
  --primary-capability-id "${primary_capability_id}"
  --command "${command_executed}"
  --category "agent_policy"
  --verb "inspect"
  --target "env sandbox marker"
  --status "${status}"
  --errno "${errno_value}"
  --message "${message}"
  --raw-exit-code "${raw_exit_code}"
  --operation-arg "grep_pattern" "${regex}"
  --operation-arg-list "candidate_vars" "${candidate_vars[*]}"
  --payload-stdout "${stdout_text}"
  --payload-stderr "${stderr_text}"
  --payload-raw-list "candidate_vars" "${candidate_vars[*]}"
  --payload-raw-field "stdout" "${stdout_text}"
  --payload-raw-field "stderr" "${stderr_text}"
)

if [[ -n "${detected_var}" ]]; then
  emit_cmd+=(--payload-raw-field "detected_var" "${detected_var}")
else
  emit_cmd+=(--payload-raw-null "detected_var")
fi

if [[ -n "${detected_value}" ]]; then
  emit_cmd+=(--payload-raw-field "detected_value" "${detected_value}")
else
  emit_cmd+=(--payload-raw-null "detected_value")
fi

"${emit_cmd[@]}"
