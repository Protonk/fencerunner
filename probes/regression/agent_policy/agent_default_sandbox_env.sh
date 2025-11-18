#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="agent_default_sandbox_env"
primary_capability_id="cap_agent_default_sandboxing"

candidate_vars=(
  "CODEX_SANDBOX"
  "CODEX_SANDBOX_ENV_VAR"
)

regex='^(CODEX_SANDBOX|CODEX_SANDBOX_ENV_VAR)='
printf -v command_executed "env | grep -m 1 -E %q" "${regex}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="partial"
errno_value=""
message="No sandbox env marker detected"
raw_exit_code=""

set +e
env | grep -m 1 -E "${regex}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

detected_var=""
detected_value=""

if [[ ${exit_code} -eq 0 ]]; then
  first_line="${stdout_text%%$'\n'*}"
  detected_var="${first_line%%=*}"
  detected_value="${first_line#*=}"
  status="success"
  message="Sandbox env marker ${detected_var}=${detected_value}"
elif [[ ${exit_code} -eq 1 ]]; then
  status="partial"
  message="Sandbox env marker absent"
else
  status="error"
  message="env grep for sandbox marker failed with exit code ${exit_code}"
fi

raw_payload=$(jq -n \
  --arg detected_var "${detected_var}" \
  --arg detected_value "${detected_value}" \
  --arg stdout "${stdout_text}" \
  --arg stderr "${stderr_text}" \
  --argjson candidate_vars "$(printf '%s\n' "${candidate_vars[@]}" | jq -R . | jq -s .)" \
  '{detected_var: ($detected_var | if length > 0 then . else null end),
    detected_value: ($detected_value | if length > 0 then . else null end),
    candidate_vars: $candidate_vars,
    stdout: $stdout,
    stderr: $stderr}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg regex "${regex}" \
  --argjson candidate_vars "$(printf '%s\n' "${candidate_vars[@]}" | jq -R . | jq -s .)" \
  '{grep_pattern: $regex, candidate_vars: $candidate_vars}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "agent_policy" \
  --verb "inspect" \
  --target "env sandbox marker" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
