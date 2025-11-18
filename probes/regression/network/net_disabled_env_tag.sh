#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="net_disabled_env_tag"
primary_capability_id="cap_net_disabled_with_tag"
target_url="${FENCE_NETWORK_TEST_URL:-https://example.com/}"
timeout_seconds="${FENCE_NETWORK_TEST_TIMEOUT:-5}"
if ! [[ "${timeout_seconds}" =~ ^[0-9]+$ ]]; then
  timeout_seconds=5
fi

printf -v command_executed "curl -m %q -s -o /dev/null -w %%{http_code} -I %q" "${timeout_seconds}" "${target_url}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
curl -m "${timeout_seconds}" -s -o /dev/null -w "%{http_code}" -I "${target_url}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

network_disabled_flag="${CODEX_SANDBOX_NETWORK_DISABLED:-}"
lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="External HEAD request succeeded"
else
  if [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Network operation blocked: operation not permitted"
  elif [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Network operation blocked: permission denied"
  elif [[ -n "${network_disabled_flag}" ]]; then
    status="denied"
    errno_value="ENETUNREACH"
    message="Network disabled marker present; request blocked"
  else
    status="error"
    errno_value=""
    message="HEAD request failed with exit code ${exit_code}"
  fi
fi

raw_json=$(jq -n \
  --arg network_disabled_env "${network_disabled_flag}" \
  --arg http_status "${stdout_text}" \
  --argjson timeout "${timeout_seconds}" \
  --arg url "${target_url}" \
  --argjson curl_exit "${exit_code}" \
  '{network_disabled_env: ($network_disabled_env | if length > 0 then . else null end),
    http_status: ($http_status | if length > 0 then . else null end),
    url: $url,
    timeout_seconds: $timeout,
    curl_exit_code: $curl_exit}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_json}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg method "HEAD" \
  --arg url "${target_url}" \
  --argjson timeout "${timeout_seconds}" \
  '{method: $method, url: $url, timeout_seconds: $timeout}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "net" \
  --verb "connect" \
  --target "${target_url}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
