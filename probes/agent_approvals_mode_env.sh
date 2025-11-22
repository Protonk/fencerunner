#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="agent_approvals_mode_env"
primary_capability_id="cap_agent_approvals_mode"

candidate_vars=(
  "FENCE_APPROVALS_MODE"
  "CODEX_APPROVALS_MODE"
  "CODEX_APPROVAL_MODE"
  "CODEX_APPROVALS"
  "CODEX_PERMISSIONS_MODE"
)

regex='^(FENCE_APPROVALS_MODE|CODEX_APPROVALS_MODE|CODEX_APPROVAL_MODE|CODEX_APPROVALS|CODEX_PERMISSIONS_MODE)='
printf -v command_executed "env | grep -m 1 -E %q" "${regex}"

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

status="partial"
errno_value=""
message="No approvals mode indicator found"
raw_exit_code=""

stdout_text=""
stderr_text=""
set +e
command_output=$(env | grep -m 1 -E "${regex}" 2>&1)
exit_code=$?
set -e

if [[ ${exit_code} -eq 0 ]]; then
  stdout_text="${command_output}"
  stderr_text=""
else
  stdout_text=""
  stderr_text="${command_output}"
fi

raw_exit_code="${exit_code}"

detected_var=""
detected_value=""

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Approvals mode indicator present"
  first_line="${stdout_text%%$'\n'*}"
  detected_var="${first_line%%=*}"
  detected_value="${first_line#*=}"
elif [[ ${exit_code} -eq 1 ]]; then
  status="partial"
  message="No approvals mode env variable present"
else
  status="denied"
  errno_value=$(detect_errno_from_text "${stderr_text}")
  message="env grep for approvals mode blocked with exit code ${exit_code}"
fi

emit_args=(
  --run-mode "${run_mode}"
  --probe-name "${probe_name}"
  --probe-version "1"
  --primary-capability-id "${primary_capability_id}"
  --command "${command_executed}"
  --category "agent_policy"
  --verb "inspect"
  --target "env approvals mode"
  --status "${status}"
  --errno "${errno_value}"
  --message "${message}"
  --raw-exit-code "${raw_exit_code}"
  --operation-arg "grep_pattern" "${regex}"
  --operation-arg-list "candidate_vars" "${candidate_vars[*]}"
  --payload-stdout "${stdout_text}"
  --payload-stderr "${stderr_text}"
  --payload-raw-field "stdout" "${stdout_text}"
  --payload-raw-field "stderr" "${stderr_text}"
  --payload-raw-list "candidate_vars" "${candidate_vars[*]}"
)

if [[ -n "${detected_var}" ]]; then
  emit_args+=(--payload-raw-field "detected_var" "${detected_var}")
else
  emit_args+=(--payload-raw-null "detected_var")
fi

if [[ -n "${detected_value}" ]]; then
  emit_args+=(--payload-raw-field "detected_value" "${detected_value}")
else
  emit_args+=(--payload-raw-null "detected_value")
fi

"${emit_record_bin}" "${emit_args[@]}"
