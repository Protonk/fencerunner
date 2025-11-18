#!/usr/bin/env bash
set -euo pipefail

# Probe for cap_fs_read_user_content via a workspace symlink pointing to ~/Desktop.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_user_desktop_symlink_read"
probe_version="1"
primary_capability_id="cap_fs_read_user_content"
secondary_capability_id="cap_fs_follow_symlinks_out_of_workspace"
target_dir="${FENCE_USER_DESKTOP_PATH:-${HOME}/Desktop}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
probe_dir=$(mktemp -d "${repo_root}/tmp_desktop_symlink.XXXXXX")
symlink_path="${probe_dir}/desktop_link"
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"
  rm -rf "${probe_dir}"
}
trap cleanup EXIT

if [[ ! -d "${target_dir}" ]]; then
  printf 'Target Desktop directory not found at %s\n' "${target_dir}" >"${stderr_tmp}"
  status="error"
  errno_value="ENOENT"
  message="Desktop directory missing"
  raw_exit_code="1"
  stdout_text=""
  stderr_text=$(tr -d '\0' <"${stderr_tmp}")
else
  ln -sf "${target_dir}" "${symlink_path}"
  printf -v command_executed "ls -ld %q" "${symlink_path}"

  status="error"
  errno_value=""
  message=""
  raw_exit_code=""

  set +e
  ls -ld "${symlink_path}" >"${stdout_tmp}" 2>"${stderr_tmp}"
  exit_code=$?
  set -e
  raw_exit_code="${exit_code}"
  stdout_text=$(tr -d '\0' <"${stdout_tmp}")
  stderr_text=$(tr -d '\0' <"${stderr_tmp}")
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

  if [[ ${exit_code} -eq 0 ]]; then
    status="success"
    message="Listed Desktop via symlink"
  elif [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied reading Desktop via symlink"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted reading Desktop via symlink"
  elif [[ "${lower_err}" == *"no such file or directory"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="Desktop target missing"
  else
    status="error"
    message="ls -ld failed with exit code ${exit_code}"
  fi
fi

if [[ -z "${command_executed:-}" ]]; then
  printf -v command_executed "ls -ld %q" "${symlink_path}"
fi

jq -n \
  --arg stdout_snippet "${stdout_text:-}" \
  --arg stderr_snippet "${stderr_text:-}" \
  --arg symlink_path "${symlink_path}" \
  --arg target_dir "${target_dir}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: {symlink_path: $symlink_path, target_dir: $target_dir}}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg symlink_path "${symlink_path}" \
  --arg target_dir "${target_dir}" \
  '{path_type: "directory", read_type: "list", via_symlink: true,
    symlink_path: $symlink_path, target_dir: $target_dir}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --secondary-capability-id "${secondary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "read" \
  --target "${symlink_path}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
