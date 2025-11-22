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
probe_name="fs_workspace_relative_segments_write_ok"
probe_version="1"
primary_capability_id="cap_fs_write_workspace_tree"

relative_root=$(mktemp -d "${repo_root}/tmp/relative_segments.XXXXXX")
mkdir -p "${relative_root}/a/b/c"
relative_path="${relative_root#"${repo_root}/"}/a/b/../b/./c/../c/target.txt"
attempt_path="${repo_root}/${relative_path}"

payload_content="relative segments write $(date -u +%Y-%m-%dT%H:%M:%SZ) $$"
printf -v command_executed "printf %q > %q" "${payload_content}" "${attempt_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}"
  rm -rf "${relative_root}"
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
if [[ -f "${attempt_path}" ]]; then
  read_back=$(tr -d '\0' <"${attempt_path}")
fi

canonical_path=$(portable_realpath "${attempt_path}")

content_match="false"
if [[ "${read_back}" == "${payload_content}" ]]; then
  content_match="true"
fi

if [[ ${exit_code} -eq 0 && "${content_match}" == "true" ]]; then
  status="success"
  message="Relative segments resolved inside workspace"
elif [[ ${exit_code} -eq 0 ]]; then
  status="partial"
  message="Relative path write succeeded but read mismatch"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied following relative segments"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted for relative segments"
  elif [[ "${lower_err}" == *"read-only file system"* ]]; then
    status="denied"
    errno_value="EROFS"
    message="Workspace reported read-only"
  elif [[ "${lower_err}" == *"no such file"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="Relative segments target missing"
  else
    status="error"
    message="Relative segments write failed with exit code ${exit_code}"
  fi
fi

relative_display="${relative_path}"
data_length=${#payload_content}

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "write_then_read_file" \
  --target "${relative_display}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "relative_path" "${relative_display}" \
  --payload-raw-field "absolute_path" "${attempt_path}" \
  --payload-raw-field "canonical_path" "${canonical_path}" \
  --payload-raw-field "data_written" "${payload_content}" \
  --payload-raw-field "data_read" "${read_back}" \
  --payload-raw-field-json "contents_match" "${content_match}" \
  --operation-arg "relative_path_used" "${relative_display}" \
  --operation-arg "canonical_path" "${canonical_path}" \
  --operation-arg-json "write_then_read_bytes" "${data_length}" \
  --operation-arg-json "verified_match" "${content_match}"
