#!/usr/bin/env bash
set -euo pipefail

# Targets cap_net_outbound_any: attempts an HTTPS request to example.com to see if outbound network is allowed.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="net_outbound_example_https"
primary_capability_id="cap_net_outbound_any"
target_url="${FENCE_NET_OUTBOUND_URL:-https://example.com}"
connect_timeout="5"
max_time="10"
printf -v command_executed "curl -sS --fail --show-error --connect-timeout %q --max-time %q -o /dev/null %q" \
  "${connect_timeout}" "${max_time}" "${target_url}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""
network_disabled_env="${CODEX_SANDBOX_NETWORK_DISABLED:-}" 

set +e
curl -sS --fail --show-error --connect-timeout "${connect_timeout}" --max-time "${max_time}" \
  -o /dev/null "${target_url}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="HTTPS request succeeded"
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
  elif [[ "${lower_err}" == *"ssl certificate problem"* ]]; then
    status="error"
    errno_value="EPROTO"
    message="TLS handshake failed"
  fi
fi

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
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "target_url" "${target_url}" \
  --payload-raw-field "network_disabled_env" "${network_disabled_env}" \
  --operation-arg "url" "${target_url}" \
  --operation-arg "method" "GET" \
  --operation-arg-json "timeout_seconds" "${max_time}" \
  --operation-arg "tool" "curl"
