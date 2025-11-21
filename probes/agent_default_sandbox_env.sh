#!/usr/bin/env bash
set -euo pipefail

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="agent_default_sandbox_env"
primary_capability_id="cap_agent_default_sandboxing"

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
mkdir -p "${repo_root}/tmp"

payload_tmp=""
cleanup_payload() {
  if [[ -n "${payload_tmp}" && -f "${payload_tmp}" ]]; then
    rm -f "${payload_tmp}"
  fi
}
trap cleanup_payload EXIT

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

candidate_json=$(printf '%s\n' "${candidate_vars[@]}" | jq -R . | jq -s .)
raw_payload=$(jq -n \
  --arg detected_var "${detected_var}" \
  --arg detected_value "${detected_value}" \
  --arg stdout "${stdout_text}" \
  --arg stderr "${stderr_text}" \
  --argjson candidate_vars "${candidate_json}" \
  '{detected_var: ($detected_var | if length > 0 then . else null end),
    detected_value: ($detected_value | if length > 0 then . else null end),
    candidate_vars: $candidate_vars,
    stdout: $stdout,
    stderr: $stderr}')

payload_json=$(jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}')

operation_args=$(jq -n \
  --arg regex "${regex}" \
  --argjson candidate_vars "${candidate_json}" \
  '{grep_pattern: $regex, candidate_vars: $candidate_vars}')

payload_file_args=()
set +e
mktemp_output=$(mktemp "${repo_root}/tmp/${probe_name}.payload.XXXXXX" 2>&1)
mktemp_status=$?
set -e
if [[ ${mktemp_status} -eq 0 ]]; then
  payload_tmp="${mktemp_output//$'\n'/}"
  printf '%s' "${payload_json}" >"${payload_tmp}"
  payload_file_args=(--payload-file "${payload_tmp}")
else
  mktemp_error=$(printf '%s' "${mktemp_output}" | tr -d '\n')
  if [[ -z "${errno_value}" ]]; then
    if [[ "${mktemp_error}" == *"Operation not permitted"* ]]; then
      errno_value="EPERM"
    elif [[ "${mktemp_error}" == *"Permission denied"* ]]; then
      errno_value="EACCES"
    fi
  fi
  status="denied"
  if [[ -n "${message}" ]]; then
    message+="; "
  fi
  message+="Sandbox denied payload scratch file: ${mktemp_error}"
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
  --operation-args "${operation_args}"
)

if [[ ${#payload_file_args[@]} -gt 0 ]]; then
  emit_cmd+=("${payload_file_args[@]}")
fi

"${emit_cmd[@]}"
