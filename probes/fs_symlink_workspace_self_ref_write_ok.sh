#!/usr/bin/env bash
set -euo pipefail

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
probe_name="fs_symlink_workspace_self_ref_write_ok"
probe_version="1"
primary_capability_id="cap_fs_write_workspace_tree"
secondary_capability_id="cap_fs_follow_symlinks_out_of_workspace"

selflink_root=$(mktemp -d "${repo_root}/tmp/selflink_loop.XXXXXX")
mkdir -p "${selflink_root}/selflink"
loop_link="${selflink_root}/selflink/loop"
if [[ -e "${loop_link}" ]]; then
  rm -rf "${loop_link}"
fi
ln -s "../selflink" "${loop_link}"

target_file="${selflink_root}/selflink/target.txt"
touch "${target_file}"

relative_dir="${selflink_root#"${repo_root}/"}/selflink"
attempt_relative_path="${relative_dir}/loop/loop/target.txt"
attempt_path="${repo_root}/${attempt_relative_path}"

payload_content="symlink self-ref write $(date -u +%Y-%m-%dT%H:%M:%SZ) $$"
printf -v command_executed "printf %q > %q" "${payload_content}" "${attempt_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}"
  rm -rf "${selflink_root}"
}
trap cleanup EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
bash -c 'printf "%s\n" "$1" > "$2"' _ "${payload_content}" "${attempt_path}" \
  >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
read_back=""
if [[ -f "${target_file}" ]]; then
  read_back=$(tr -d '\0' <"${target_file}")
fi

canonical_path=$(portable_realpath "${attempt_path}")

content_match="false"
if [[ "${read_back}" == "${payload_content}" ]]; then
  content_match="true"
fi

if [[ ${exit_code} -eq 0 && "${content_match}" == "true" ]]; then
  status="success"
  message="Symlink self-reference resolved inside workspace"
elif [[ ${exit_code} -eq 0 ]]; then
  status="partial"
  message="Symlink path write succeeded but read mismatch"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied writing via self-referential symlink"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted following symlink loop"
  elif [[ "${lower_err}" == *"symbolic link loop"* ]]; then
    status="error"
    errno_value="ELOOP"
    message="Kernel reported symlink loop"
  else
    status="error"
    message="Symlink write failed with exit code ${exit_code}"
  fi
fi

data_length=${#payload_content}

verified_bool="false"
if [[ "${content_match}" == "true" ]]; then
  verified_bool="true"
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --secondary-capability-id "${secondary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "write_then_read_file_via_symlink" \
  --target "${attempt_relative_path}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "relative_path" "${attempt_relative_path}" \
  --payload-raw-field "absolute_path" "${attempt_path}" \
  --payload-raw-field "canonical_path" "${canonical_path}" \
  --payload-raw-field "target_file" "${target_file}" \
  --payload-raw-field "data_written" "${payload_content}" \
  --payload-raw-field "data_read" "${read_back}" \
  --payload-raw-field-json "contents_match" "${content_match}" \
  --payload-raw-field "symlink_path" "${loop_link}" \
  --payload-raw-field "symlink_target" "../selflink" \
  --operation-arg "relative_path" "${attempt_relative_path}" \
  --operation-arg "canonical_path" "${canonical_path}" \
  --operation-arg "symlink_path" "${loop_link}" \
  --operation-arg "symlink_target" "../selflink" \
  --operation-arg-json "write_then_read_verified" "${verified_bool}"
