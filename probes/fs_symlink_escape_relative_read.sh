#!/usr/bin/env bash
set -euo pipefail

# Variant for cap_fs_follow_symlinks_out_of_workspace: uses a relative symlink chain to reach /etc/hosts.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
helpers_lib="${repo_root}/tools/lib/helpers.sh"
# shellcheck source=tools/lib/helpers.sh
source "${helpers_lib}"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_symlink_escape_relative_read"
primary_capability_id="cap_fs_follow_symlinks_out_of_workspace"
real_target="/etc/hosts"
probe_dir=$(mktemp -d "${repo_root}/tmp_symlink_escape_rel.XXXXXX")
inner_dir="${probe_dir}/inner"
mkdir -p "${inner_dir}"
symlink_path="${inner_dir}/hosts_relative"
relative_target=$(portable_relpath "${real_target}" "${inner_dir}")
printf -v command_executed "head -n 1 %q" "${symlink_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"
  rm -rf "${probe_dir}"
}
trap cleanup EXIT

ln -sf "${relative_target}" "${symlink_path}"

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
head -n 1 "${symlink_path}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
stdout_text=$(tr -d '\0' <"${stdout_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Read via relative symlink"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied following relative symlink"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted following relative symlink"
  elif [[ "${lower_err}" == *"no such file or directory"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="Relative target missing"
  else
    status="error"
    message="Relative symlink read failed with exit code ${exit_code}"
  fi
fi

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --arg symlink_path "${symlink_path}" \
  --arg real_target "${real_target}" \
  --arg relative_target "${relative_target}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: {symlink_path: $symlink_path, real_target: $real_target, relative_target: $relative_target}}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg symlink_target "${real_target}" \
  --arg relative_target "${relative_target}" \
  '{symlink_target: $symlink_target, relative_target: $relative_target, attempt_via_symlink: true}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
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
