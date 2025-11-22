#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
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
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

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

network_env_flag=(--payload-raw-null "network_disabled_env")
if [[ -n "${network_disabled_flag}" ]]; then
  network_env_flag=(--payload-raw-field "network_disabled_env" "${network_disabled_flag}")
fi

http_status_flag=(--payload-raw-null "http_status")
if [[ -n "${stdout_text}" ]]; then
  http_status_flag=(--payload-raw-field "http_status" "${stdout_text}")
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
  --payload-raw-field "url" "${target_url}" \
  --payload-raw-field-json "timeout_seconds" "${timeout_seconds}" \
  --payload-raw-field-json "curl_exit_code" "${exit_code}" \
  "${network_env_flag[@]}" \
  "${http_status_flag[@]}" \
  --operation-arg "method" "HEAD" \
  --operation-arg "url" "${target_url}" \
  --operation-arg-json "timeout_seconds" "${timeout_seconds}"
