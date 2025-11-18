#!/usr/bin/env bash
set -euo pipefail

# Attempt an HTTP HEAD against an IPv4 literal to observe network-disabled enforcement.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="net_outbound_ipv4_literal_denied"
probe_version="1"
primary_capability_id="cap_net_outbound_any"
secondary_capability_id="cap_net_disabled_with_tag"

curl_bin="${FENCE_CURL_BIN:-/usr/bin/curl}"
target_url="${FENCE_NET_IPV4_LITERAL_URL:-http://1.1.1.1}"
max_time="${FENCE_CURL_MAX_TIME:-5}"
connect_timeout="${FENCE_CURL_CONNECT_TIMEOUT:-3}"
network_disabled_marker="${CODEX_SANDBOX_NETWORK_DISABLED:-}"

printf -v command_executed "%q -I --max-time %s --connect-timeout %s -sS %q" \
  "${curl_bin}" "${max_time}" "${connect_timeout}" "${target_url}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"
}
trap cleanup EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

if [[ ! -x "${curl_bin}" ]]; then
  status="error"
  errno_value="ENOENT"
  message="curl binary missing"
  raw_exit_code="1"
  stdout_text=""
  stderr_text="curl not found at ${curl_bin}"
else
  set +e
  "${curl_bin}" -I --max-time "${max_time}" --connect-timeout "${connect_timeout}" -sS \
    "${target_url}" >"${stdout_tmp}" 2>"${stderr_tmp}"
  exit_code=$?
  set -e

  raw_exit_code="${exit_code}"
  stdout_text=$(tr -d '\0' <"${stdout_tmp}")
  stderr_text=$(tr -d '\0' <"${stderr_tmp}")
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

  if [[ ${exit_code} -eq 0 ]]; then
    status="success"
    message="IPv4 literal head request succeeded"
  elif [[ ${exit_code} -eq 7 ]] || [[ "${lower_err}" == *"failed to connect"* ]] || [[ "${lower_err}" == *"couldn't connect"* ]]; then
    errno_value="ECONNREFUSED"
    if [[ -n "${network_disabled_marker}" ]]; then
      status="denied"
      message="IPv4 literal blocked while network disabled marker set"
    else
      status="partial"
      message="IPv4 literal blocked but network disabled marker missing"
    fi
  elif [[ ${exit_code} -eq 6 ]] || [[ "${lower_err}" == *"could not resolve host"* ]]; then
    status="error"
    errno_value="EAI_NONAME"
    message="curl reported DNS failure for IP literal"
  else
    status="error"
    message="curl IPv4 literal request failed with exit code ${exit_code}"
  fi
fi

truncate() {
  local value="$1"
  if [[ ${#value} -gt 400 ]]; then
    printf '%s…' "${value:0:400}"
  else
    printf '%s' "${value}"
  fi
}

stdout_snippet=$(truncate "${stdout_text:-}")
stderr_snippet=$(truncate "${stderr_text:-}")

raw_payload=$(jq -n \
  --arg target_url "${target_url}" \
  --arg stdout "${stdout_text:-}" \
  --arg stderr "${stderr_text:-}" \
  --arg curl_bin "${curl_bin}" \
  --arg marker "${network_disabled_marker}" \
  --argjson exit_code "${raw_exit_code:-0}" \
  '{target_url: $target_url,
    curl_bin: $curl_bin,
    network_disabled_marker: ($marker | if length > 0 then $marker else null end),
    stdout: (if ($stdout | length) > 0 then $stdout else null end),
    stderr: (if ($stderr | length) > 0 then $stderr else null end),
    exit_code: $exit_code}')

jq -n \
  --arg stdout_snippet "${stdout_snippet}" \
  --arg stderr_snippet "${stderr_snippet}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: (if ($stdout_snippet | length) > 0 then $stdout_snippet else "" end),
    stderr_snippet: (if ($stderr_snippet | length) > 0 then $stderr_snippet else "" end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg target_url "${target_url}" \
  --arg method "HTTP_HEAD" \
  --arg literal_ip "true" \
  --argjson max_time "${max_time}" \
  --argjson connect_timeout "${connect_timeout}" \
  --arg marker "${network_disabled_marker}" \
  '{target_url: $target_url,
    method: $method,
    literal_ip: ($literal_ip == "true"),
    max_time_seconds: $max_time,
    connect_timeout_seconds: $connect_timeout,
    network_disabled_marker: ($marker | if length > 0 then $marker else null end)}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --secondary-capability-id "${secondary_capability_id}" \
  --command "${command_executed}" \
  --category "network" \
  --verb "connect" \
  --target "${target_url}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
