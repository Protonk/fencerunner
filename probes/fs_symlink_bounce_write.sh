#!/usr/bin/env bash
set -euo pipefail

# Experiment: write through a symlink chain that bounces outside the workspace twice
# before returning to a workspace file. Targets cap_fs_follow_symlinks_out_of_workspace.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
portable_path_helper="${repo_root}/bin/portable-path"
if [[ ! -x "${portable_path_helper}" ]]; then
  echo "portable-path helper missing at ${portable_path_helper}. Run 'make build-bin'." >&2
  exit 1
fi

portable_realpath() {
  "${portable_path_helper}" realpath "$1"
}

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
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}"
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
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "attempt_path" "${attempt_path}" \
  --payload-raw-field "resolved_path" "${real_attempt_path}" \
  --payload-raw-field "target_file" "${target_file}" \
  --payload-raw-field-json "symlink_chain" "[{\"link\":\"${link_out}\",\"target\":\"${external_dir}\"},{\"link\":\"${return_middle}\",\"target\":\"${middle_dir}\"},{\"link\":\"${second_hop}\",\"target\":\"${external_dir}\"},{\"link\":\"${return_final}\",\"target\":\"${target_file}\"}]" \
  --payload-raw-field "target_tail" "${tail_snippet}" \
  --operation-arg "attempt_path" "${attempt_path}" \
  --operation-arg "target_file" "${target_file}" \
  --operation-arg-json "attempt_bytes" "${attempt_bytes}" \
  --operation-arg-json "hops" "4" \
  --operation-arg-json "writes_via_symlink_chain" "true"
