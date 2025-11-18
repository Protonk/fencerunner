#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="agent_sandbox_env_marker"
primary_capability_id="cap_agent_sandbox_env_marker"
marker_var="CODEX_SANDBOX_ENV_VAR"
printf -v command_executed "printenv %s" "${marker_var}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
printenv "${marker_var}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
marker_value=$(printf '%s' "${stdout_text}" | tr -d '\n')
fallback_marker="${CODEX_SANDBOX:-}"

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
  status="error"
  message="printenv exited with ${exit_code}"
fi

raw_payload=$(jq -n \
  --arg marker_var "${marker_var}" \
  --arg marker_value "${marker_value}" \
  --arg fallback_marker "${fallback_marker}" \
  '{marker_var: $marker_var,
    marker_value: ($marker_value | if length > 0 then . else null end),
    fallback_marker: ($fallback_marker | if length > 0 then . else null end)}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n --arg marker_var "${marker_var}" '{marker_var: $marker_var}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "agent_policy" \
  --verb "inspect" \
  --target "${marker_var}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
