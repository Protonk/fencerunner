#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="net_outbound_example_http_head"
primary_capability_id="cap_net_outbound_any"
target_url="${FENCE_NET_OUTBOUND_HTTP_URL:-http://example.com/}"
connect_timeout="5"
max_time="10"
method="HEAD"
printf -v command_executed "curl -sS -I --connect-timeout %q --max-time %q %q" \
  "${connect_timeout}" "${max_time}" "${target_url}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""
network_disabled_env="${CODEX_SANDBOX_NETWORK_DISABLED:-}"

set +e
curl -sS -I --connect-timeout "${connect_timeout}" --max-time "${max_time}" \
  "${target_url}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
status_line=$(printf '%s' "${stdout_text}" | grep -m 1 -i '^http/' || true)
http_status=""
if [[ -n "${status_line}" ]]; then
  http_status=$(printf '%s' "${status_line}" | awk '{print $2}')
fi

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  if [[ -n "${http_status}" ]]; then
    message="HTTP HEAD succeeded (${http_status})"
  else
    message="HTTP HEAD succeeded"
  fi
else
  status="denied"
  message="Network request denied"
  if [[ -n "${network_disabled_env}" ]]; then
    message="Network disabled via CODEX_SANDBOX_NETWORK_DISABLED"
  fi
  if [[ "${lower_err}" == *"could not resolve host"* ]]; then
    errno_value="EAI_AGAIN"
    message="DNS resolution failed"
  elif [[ "${lower_err}" == *"connection timed out"* ]]; then
    errno_value="ETIMEDOUT"
    message="Connection timed out"
  elif [[ "${lower_err}" == *"connection refused"* || "${lower_err}" == *"failed to connect"* ]]; then
    errno_value="ECONNREFUSED"
    message="Connection refused"
  elif [[ "${lower_err}" == *"network is unreachable"* ]]; then
    errno_value="ENETUNREACH"
    message="Network is unreachable"
  elif [[ "${lower_err}" == *"command not found"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="curl binary missing"
  fi
fi

raw_payload=$(jq -n \
  --arg stdout "${stdout_text}" \
  --arg stderr "${stderr_text}" \
  --arg status_line "${status_line}" \
  --arg http_status "${http_status}" \
  --arg target_url "${target_url}" \
  --arg method "${method}" \
  --arg network_disabled "${network_disabled_env}" \
  '{stdout: $stdout,
    stderr: $stderr,
    status_line: (if ($status_line | length) > 0 then $status_line else null end),
    http_status: (if ($http_status | length) > 0 then $http_status else null end),
    target_url: $target_url,
    method: $method,
    network_disabled_env: (if ($network_disabled | length) > 0 then $network_disabled else null end)}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg url "${target_url}" \
  --arg method "${method}" \
  --argjson connect_timeout "${connect_timeout}" \
  --argjson max_time "${max_time}" \
  '{url: $url,
    method: $method,
    connect_timeout: ($connect_timeout | tonumber),
    max_time: ($max_time | tonumber),
    tool: "curl"}')

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
