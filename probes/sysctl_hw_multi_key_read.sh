#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="sysctl_hw_multi_key_read"
probe_version="1"
primary_capability_id="cap_sysctl_read_basic"
sysctl_bin="${FENCE_SYSCTL_BIN:-/usr/sbin/sysctl}"
keys=("hw.ncpu" "hw.memsize")
command_executed=$(printf %q "${sysctl_bin}")
command_executed+=" -n"
for key in "${keys[@]}"; do
  command_executed+=" $(printf %q "${key}")"
done

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
"${sysctl_bin}" -n "${keys[@]}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

value_lines=()
while IFS= read -r line; do
  value_lines+=("${line}")
done <"${stdout_tmp}"
value_count=${#value_lines[@]}
key_count=${#keys[@]}

if [[ ${exit_code} -eq 0 && ${value_count} -eq ${key_count} ]]; then
  status="success"
  message="Read ${key_count} sysctl keys in one call"
elif [[ ${exit_code} -eq 0 ]]; then
  status="partial"
  message="sysctl returned ${value_count} values for ${key_count} keys"
elif [[ "${lower_err}" == *"permission denied"* ]]; then
  status="denied"
  errno_value="EACCES"
  message="sysctl multi-key read denied"
elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
  status="denied"
  errno_value="EPERM"
  message="sysctl multi-key read operation not permitted"
elif [[ "${lower_err}" == *"unknown oid"* ]]; then
  status="error"
  errno_value="ENOENT"
  message="sysctl key missing"
else
  status="error"
  message="sysctl multi-key read failed with exit code ${exit_code}"
fi

keys_json=$(printf '%s\n' "${keys[@]}" | jq -R . | jq -s .)
if [[ ${value_count} -gt 0 ]]; then
  values_json=$(printf '%s\n' "${value_lines[@]}" | jq -R . | jq -s .)
else
  values_json='[]'
fi

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson keys "${keys_json}" \
  --argjson values "${values_json}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: {keys: $keys, values: $values}}' >"${payload_tmp}"

operation_args=$(jq -n \
  --argjson keys "${keys_json}" \
  --arg sysctl_bin "${sysctl_bin}" \
  '{keys: $keys, mode: "multi_key", sysctl_bin: $sysctl_bin}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "sysctl" \
  --verb "read" \
  --target "hw.ncpu hw.memsize" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
