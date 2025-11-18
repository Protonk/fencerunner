#!/usr/bin/env bash
set -euo pipefail

# Experiment: write through a symlink chain that bounces outside the workspace twice
# before returning to a workspace file. Targets cap_fs_follow_symlinks_out_of_workspace.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
helpers_lib="${repo_root}/tools/lib/helpers.sh"
# shellcheck source=tools/lib/helpers.sh
source "${helpers_lib}"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_symlink_bounce_write"
probe_version="1"
primary_capability_id="cap_fs_follow_symlinks_out_of_workspace"
secondary_capability_id="cap_fs_write_workspace_tree"

workspace_chain_root=$(mktemp -d "${repo_root}/tmp_symlink_bounce.XXXXXX")
middle_dir="${workspace_chain_root}/middle_hop"
target_dir="${workspace_chain_root}/final_destination"
mkdir -p "${middle_dir}" "${target_dir}"
target_file="${target_dir}/boomerang.txt"
printf 'initial\n' >"${target_file}"

external_dir=$(mktemp -d "/tmp/codex_symlink_bounce_external.XXXXXX")
link_out="${workspace_chain_root}/hop_outside"
ln -s "${external_dir}" "${link_out}"

return_middle="${external_dir}/return_middle"
ln -s "${middle_dir}" "${return_middle}"

second_hop="${middle_dir}/hop_outside_again"
ln -s "${external_dir}" "${second_hop}"

return_final="${external_dir}/return_final"
ln -s "${target_file}" "${return_final}"

attempt_path="${link_out}/return_middle/hop_outside_again/return_final"
timestamp=$(date -u +"%Y%m%dT%H%M%SZ")
attempt_line="symlink bounce attempt ${timestamp}"
attempt_bytes=$(( ${#attempt_line} + 1 ))
printf -v command_executed "printf %q >> %q" "${attempt_line}" "${attempt_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"
  rm -rf "${workspace_chain_root}" "${external_dir}"
}
trap cleanup EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
{
  printf '%s\n' "${attempt_line}" >>"${attempt_path}"
} >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

tail_snippet=$(tail -n 5 "${target_file}" 2>/dev/null || true)
tail_snippet=$(printf '%s' "${tail_snippet}" | tr -d '\0')

real_attempt_path=$(portable_realpath "${attempt_path}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Appended via multi-hop symlink bounce"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied following bounce chain"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted following bounce chain"
  elif [[ "${lower_err}" == *"no such file or directory"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="Bounce chain target missing"
  else
    status="error"
    message="Symlink bounce write failed with exit code ${exit_code}"
  fi
fi

truncate_field() {
  local value="$1"
  if [[ ${#value} -gt 400 ]]; then
    printf '%s…' "${value:0:400}"
  else
    printf '%s' "${value}"
  fi
}

stdout_snippet=$(truncate_field "${stdout_text}")
stderr_snippet=$(truncate_field "${stderr_text}")
tail_snippet=$(truncate_field "${tail_snippet}")

raw_payload=$(jq -n \
  --arg attempt_path "${attempt_path}" \
  --arg real_attempt_path "${real_attempt_path}" \
  --arg link_out "${link_out}" \
  --arg link_out_target "${external_dir}" \
  --arg return_middle "${return_middle}" \
  --arg return_middle_target "${middle_dir}" \
  --arg second_hop "${second_hop}" \
  --arg second_hop_target "${external_dir}" \
  --arg return_final "${return_final}" \
  --arg return_final_target "${target_file}" \
  --arg tail_snippet "${tail_snippet}" \
  '{
    attempt_path: $attempt_path,
    resolved_path: $real_attempt_path,
    symlink_chain: [
      {link: $link_out, target: $link_out_target},
      {link: $return_middle, target: $return_middle_target},
      {link: $second_hop, target: $second_hop_target},
      {link: $return_final, target: $return_final_target}
    ],
    target_tail: (if ($tail_snippet | length) > 0 then $tail_snippet else null end)
  }')

jq -n \
  --arg stdout_snippet "${stdout_snippet}" \
  --arg stderr_snippet "${stderr_snippet}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: (if ($stdout_snippet | length) > 0 then $stdout_snippet else "" end),
    stderr_snippet: (if ($stderr_snippet | length) > 0 then $stderr_snippet else "" end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg attempt_path "${attempt_path}" \
  --arg target_file "${target_file}" \
  --argjson attempt_bytes "${attempt_bytes}" \
  '{
    attempt_path: $attempt_path,
    target_file: $target_file,
    attempt_bytes: $attempt_bytes,
    hops: 4,
    writes_via_symlink_chain: true
  }')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --secondary-capability-id "${secondary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "write" \
  --target "${attempt_path}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
