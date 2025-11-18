#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="proc_unsandboxed_escalation_retry"
primary_capability_id="cap_proc_unsandboxed_escalation"
target_path="${FENCE_ESCALATION_TEST_PATH:-/System/Library/codex-fence-escalation-test}"
attempt_line="codex-fence escalation probe $(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf -v command_executed "printf %q >> %q" "${attempt_line}" "${target_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
bash -c 'printf "%s\n" "$1" >> "$2"' _ "${attempt_line}" "${target_path}" \
  >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Write outside workspace succeeded (no denial observed)"
else
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied and no unsandboxed retry detected"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted and no unsandboxed retry detected"
  elif [[ "${lower_err}" == *"read-only file system"* ]]; then
    status="denied"
    errno_value="EROFS"
    message="Target filesystem is read-only; unsandboxed retry not observed"
  else
    status="error"
    errno_value=""
    message="Write failed with exit code ${exit_code}"
  fi
fi

sandbox_mode="${FENCE_SANDBOX_MODE:-}"
escalation_marker="${CODEX_SANDBOX_ESCALATION_ALLOWED:-}"

denial_reason=""
if [[ -n "${lower_err}" ]]; then
  denial_reason="${lower_err}"
fi

raw_json=$(jq -n \
  --arg sandbox_mode "${sandbox_mode}" \
  --arg escalation_marker "${escalation_marker}" \
  --arg stderr_lower "${denial_reason}" \
  '{sandbox_mode: ($sandbox_mode | if length > 0 then . else null end),
    escalation_marker: ($escalation_marker | if length > 0 then . else null end),
    denial_diagnostics: ($stderr_lower | if length > 0 then . else null end)}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_json}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg write_mode "append" \
  --argjson attempt_bytes "${#attempt_line}" \
  '{write_mode: $write_mode, attempt_bytes: $attempt_bytes}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "proc" \
  --verb "exec" \
  --target "${target_path}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
