#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
helpers_lib="${repo_root}/tools/lib/helpers.sh"
# shellcheck source=tools/lib/helpers.sh
source "${helpers_lib}"

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
payload_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"
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

symlink_info=$(jq -n \
  --arg loop_path "${loop_link}" \
  --arg loop_target "../selflink" \
  '{link: $loop_path, target: $loop_target}')

raw_payload=$(jq -n \
  --arg relative_path "${attempt_relative_path}" \
  --arg absolute_path "${attempt_path}" \
  --arg canonical_path "${canonical_path}" \
  --arg target_file "${target_file}" \
  --arg data_written "${payload_content}" \
  --arg data_read "${read_back}" \
  --argjson symlink "${symlink_info}" \
  '{relative_path: $relative_path,
    absolute_path: $absolute_path,
    canonical_path: $canonical_path,
    target_file: $target_file,
    symlink: $symlink,
    data_written: $data_written,
    data_read: $data_read,
    contents_match: ($data_written == $data_read)}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

verified_bool="false"
if [[ "${content_match}" == "true" ]]; then
  verified_bool="true"
fi
operation_args=$(jq -n \
  --arg relative_path "${attempt_relative_path}" \
  --arg canonical_path "${canonical_path}" \
  --arg loop_link "${loop_link}" \
  --arg loop_target "../selflink" \
  --argjson verified "${verified_bool}" \
  '{relative_path: $relative_path,
    canonical_path: $canonical_path,
    symlink: {path: $loop_link, target: $loop_target},
    write_then_read_verified: $verified}')

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
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
