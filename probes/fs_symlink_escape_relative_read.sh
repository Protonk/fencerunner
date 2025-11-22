#!/usr/bin/env bash
set -euo pipefail

# Variant for cap_fs_follow_symlinks_out_of_workspace: uses a relative symlink chain to reach /etc/hosts.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
portable_path_helper="${repo_root}/bin/portable-path"
if [[ ! -x "${portable_path_helper}" ]]; then
  echo "portable-path helper missing at ${portable_path_helper}. Run 'make build-bin'." >&2
  exit 1
fi

portable_relpath() {
  "${portable_path_helper}" relpath "$1" "$2"
}

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
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}"
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
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "symlink_path" "${symlink_path}" \
  --payload-raw-field "real_target" "${real_target}" \
  --payload-raw-field "relative_target" "${relative_target}" \
  --operation-arg "symlink_target" "${real_target}" \
  --operation-arg "relative_target" "${relative_target}" \
  --operation-arg-json "attempt_via_symlink" "true"
