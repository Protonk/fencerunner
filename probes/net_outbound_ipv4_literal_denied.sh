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
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}"
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
  --payload-stdout "${stdout_text:-}" \
  --payload-stderr "${stderr_text:-}" \
  --payload-raw-field "target_url" "${target_url}" \
  --payload-raw-field "curl_bin" "${curl_bin}" \
  --payload-raw-field "network_disabled_marker" "${network_disabled_marker}" \
  --payload-raw-field-json "exit_code" "${raw_exit_code:-0}" \
  --operation-arg "target_url" "${target_url}" \
  --operation-arg "method" "HTTP_HEAD" \
  --operation-arg-json "literal_ip" "true" \
  --operation-arg-json "max_time_seconds" "${max_time}" \
  --operation-arg-json "connect_timeout_seconds" "${connect_timeout}" \
  --operation-arg "network_disabled_marker" "${network_disabled_marker}"
