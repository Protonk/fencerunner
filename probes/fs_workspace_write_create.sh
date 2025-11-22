#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_workspace_write_create"
primary_capability_id="cap_fs_write_workspace_tree"
attempt_line="codex-fence workspace write $(date -u +%Y-%m-%dT%H:%M:%SZ) $$"
target_path=$(mktemp "${repo_root}/.codex-fence-workspace-write.XXXXXX")
printf -v command_executed "printf %q > %q" "${attempt_line}" "${target_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${target_path}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
bash -c 'printf "%s\n" "$1" > "$2"' _ "${attempt_line}" "${target_path}" \
  >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
file_contents=""
if [[ -f "${target_path}" ]]; then
  file_contents=$(tr -d '\0' <"${target_path}")
fi

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Created workspace file"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied writing workspace file"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted"
  elif [[ "${lower_err}" == *"read-only file system"* ]]; then
    status="denied"
    errno_value="EROFS"
    message="Workspace write hit read-only filesystem"
  else
    status="error"
    message="Workspace write failed with exit code ${exit_code}"
  fi
fi

relative_path=""
if [[ "${target_path}" == "${repo_root}"* ]]; then
  relative_path=${target_path#"${repo_root}/"}
fi
written_bytes=${#attempt_line}

relative_payload_flags=(--payload-raw-null "relative_path")
relative_operation_flags=(--operation-arg-null "relative_path")
if [[ -n "${relative_path}" ]]; then
  relative_payload_flags=(--payload-raw-field "relative_path" "${relative_path}")
  relative_operation_flags=(--operation-arg "relative_path" "${relative_path}")
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "write" \
  --target "${target_path}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "target_path" "${target_path}" \
  --payload-raw-field "written_contents" "${file_contents}" \
  --payload-raw-field-json "written_bytes" "${written_bytes}" \
  "${relative_payload_flags[@]}" \
  --operation-arg "write_mode" "truncate" \
  --operation-arg "target_path" "${target_path}" \
  --operation-arg-json "bytes" "${written_bytes}" \
  "${relative_operation_flags[@]}"
