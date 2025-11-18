#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="sysctl_kern_boottime_read"
primary_capability_id="cap_sysctl_read_sensitive"
sysctl_bin="${FENCE_SYSCTL_BIN:-/usr/sbin/sysctl}"
target_key="${FENCE_SYSCTL_SENSITIVE_KEY:-kern.boottime}"

printf -v command_executed "%q -n %q" "${sysctl_bin}" "${target_key}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
"${sysctl_bin}" -n "${target_key}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Read ${target_key}"
elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
  status="denied"
  errno_value="EPERM"
  message="sysctl ${target_key} blocked: operation not permitted"
elif [[ "${lower_err}" == *"permission denied"* ]]; then
  status="denied"
  errno_value="EACCES"
  message="sysctl ${target_key} blocked: permission denied"
elif [[ "${lower_err}" == *"no such file or directory"* ]] || [[ "${lower_err}" == *"unknown oid"* ]]; then
  status="partial"
  errno_value="ENOENT"
  message="${target_key} unavailable"
else
  status="error"
  errno_value=""
  message="sysctl ${target_key} failed with exit code ${exit_code}"
fi

raw_payload=$(jq -n \
  --arg key "${target_key}" \
  --arg stdout "${stdout_text}" \
  --arg stderr "${stderr_text}" \
  --arg bin "${sysctl_bin}" \
  --argjson exit_code "${exit_code}" \
  '{key: $key, sysctl_bin: $bin, stdout: $stdout, stderr: $stderr, exit_code: $exit_code}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg key "${target_key}" \
  --arg sysctl_bin "${sysctl_bin}" \
  '{key: $key, sysctl_bin: $sysctl_bin}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "sysctl" \
  --verb "read" \
  --target "${target_key}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
