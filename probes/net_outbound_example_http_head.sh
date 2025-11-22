#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
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
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

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

network_disabled_field=(--payload-raw-null "network_disabled_env")
if [[ -n "${network_disabled_env}" ]]; then
  network_disabled_field=(--payload-raw-field "network_disabled_env" "${network_disabled_env}")
fi

status_line_field=(--payload-raw-null "status_line")
if [[ -n "${status_line}" ]]; then
  status_line_field=(--payload-raw-field "status_line" "${status_line}")
fi

http_status_field=(--payload-raw-null "http_status")
if [[ -n "${http_status}" ]]; then
  http_status_field=(--payload-raw-field "http_status" "${http_status}")
fi
connect_timeout_json="${connect_timeout}"
max_time_json="${max_time}"

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
  --payload-raw-field "stdout" "${stdout_text}" \
  --payload-raw-field "stderr" "${stderr_text}" \
  "${status_line_field[@]}" \
  "${http_status_field[@]}" \
  --payload-raw-field "target_url" "${target_url}" \
  --payload-raw-field "method" "${method}" \
  "${network_disabled_field[@]}" \
  --operation-arg "url" "${target_url}" \
  --operation-arg "method" "${method}" \
  --operation-arg-json "connect_timeout" "${connect_timeout_json}" \
  --operation-arg-json "max_time" "${max_time_json}" \
  --operation-arg "tool" "curl"
