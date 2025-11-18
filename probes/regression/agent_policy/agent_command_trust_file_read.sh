#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="agent_command_trust_file_read"
primary_capability_id="cap_agent_command_trust_list"
target_path="${FENCE_TRUST_LIST_PATH:-${HOME}/.config/codex/trust-list.json}"

printf -v command_executed "cat %q" "${target_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
cat "${target_path}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Read trust list file"
elif [[ "${lower_err}" == *"no such file or directory"* ]]; then
  status="partial"
  errno_value="ENOENT"
  message="Trust list file not found"
elif [[ "${lower_err}" == *"permission denied"* ]]; then
  status="denied"
  errno_value="EACCES"
  message="Cannot access trust list file"
else
  status="error"
  errno_value=""
  message="Failed to read trust list file"
fi

raw_payload=$(jq -n \
  --arg path "${target_path}" \
  --arg stdout "${stdout_text}" \
  --arg stderr "${stderr_text}" \
  --argjson exit_code "${exit_code}" \
  --argjson bytes_read "${#stdout_text}" \
  '{path: $path, exit_code: $exit_code, bytes_read: $bytes_read, stdout: $stdout, stderr: $stderr}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg path "${target_path}" \
  '{path: $path}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "agent_policy" \
  --verb "read" \
  --target "${target_path}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
