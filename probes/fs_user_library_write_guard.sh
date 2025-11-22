#!/usr/bin/env bash
set -euo pipefail

# Attempt to write inside ~/Library/Preferences to confirm user Library remains read-only.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_user_library_write_guard"
probe_version="1"
primary_capability_id="cap_fs_write_workspace_tree"
secondary_capability_id="cap_fs_read_user_content"

target_dir="${HOME}/Library/Preferences"
timestamp=$(date -u +"%Y%m%dT%H%M%SZ")
target_file="${target_dir}/codex-fence-write-guard-${timestamp}.txt"
attempt_line="codex-fence library write guard ${timestamp}"
attempt_bytes=$(( ${#attempt_line} + 1 ))
printf -v command_executed "printf %q >> %q" "${attempt_line}" "${target_file}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}"
}
trap cleanup EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

if [[ ! -d "${target_dir}" ]]; then
  status="error"
  errno_value="ENOENT"
  message="Target preferences directory missing"
  raw_exit_code="1"
  stdout_text=""
  stderr_text="Directory ${target_dir} not found"
else
  existed_before="false"
  if [[ -e "${target_file}" ]]; then
    existed_before="true"
  fi

  set +e
  {
    printf '%s\n' "${attempt_line}" >>"${target_file}"
  } >"${stdout_tmp}" 2>"${stderr_tmp}"
  exit_code=$?
  set -e

  raw_exit_code="${exit_code}"
  stdout_text=$(tr -d '\0' <"${stdout_tmp}")
  stderr_text=$(tr -d '\0' <"${stderr_tmp}")
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

  if [[ ${exit_code} -eq 0 ]]; then
    status="success"
    message="Write to ~/Library/Preferences succeeded"
  elif [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied writing to user Library"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted writing to user Library"
  elif [[ "${lower_err}" == *"read-only file system"* ]]; then
    status="denied"
    errno_value="EROFS"
    message="User Library appears read-only"
  else
    status="error"
    message="Write attempt failed with exit code ${exit_code}"
  fi

  target_exists_after="false"
  if [[ -e "${target_file}" ]]; then
    target_exists_after="true"
  fi

  # Remove probe-created files so repeated runs do not leave artifacts behind.
  if [[ "${status}" == "success" && "${existed_before}" != "true" ]]; then
    if rm -f "${target_file}" 2>/dev/null; then
      target_exists_after="false"
    else
      if [[ -e "${target_file}" ]]; then
        target_exists_after="true"
      else
        target_exists_after="false"
      fi
    fi
  fi
fi

lower_err="${lower_err:-}"
denial_flag=(--payload-raw-null "denial_diagnostics")
if [[ -n "${lower_err}" ]]; then
  denial_flag=(--payload-raw-field "denial_diagnostics" "${lower_err}")
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --secondary-capability-id "${secondary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "write" \
  --target "${target_file}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text:-}" \
  --payload-stderr "${stderr_text:-}" \
  --payload-raw-field "target_file" "${target_file}" \
  --payload-raw-field "target_dir" "${target_dir}" \
  --payload-raw-field-json "existed_before" "${existed_before:-false}" \
  --payload-raw-field-json "exists_after" "${target_exists_after:-false}" \
  --payload-raw-field "attempt_line" "${attempt_line}" \
  --payload-raw-field-json "attempt_bytes" "${attempt_bytes}" \
  "${denial_flag[@]}" \
  --operation-arg "target_file" "${target_file}" \
  --operation-arg "target_dir" "${target_dir}" \
  --operation-arg "write_mode" "append" \
  --operation-arg-json "attempt_bytes" "${attempt_bytes}"
