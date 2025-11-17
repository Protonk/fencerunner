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
payload_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"
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
fi

truncate() {
  local value="$1"
  if [[ ${#value} -gt 400 ]]; then
    printf '%s…' "${value:0:400}"
  else
    printf '%s' "${value}"
  fi
}

stdout_snippet=$(truncate "${stdout_text:-}")
stderr_snippet=$(truncate "${stderr_text:-}")

raw_payload=$(jq -n \
  --arg target_file "${target_file}" \
  --arg target_dir "${target_dir}" \
  --argjson existed_before "${existed_before:-false}" \
  --argjson exists_after "${target_exists_after:-false}" \
  --arg attempt_line "${attempt_line}" \
  --arg stderr_lower "${lower_err:-}" \
  '{target_file: $target_file,
    target_dir: $target_dir,
    existed_before: $existed_before,
    exists_after: $exists_after,
    attempt_line: $attempt_line,
    denial_diagnostics: ($stderr_lower | if length > 0 then . else null end)}')

jq -n \
  --arg stdout_snippet "${stdout_snippet}" \
  --arg stderr_snippet "${stderr_snippet}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: (if ($stdout_snippet | length) > 0 then $stdout_snippet else "" end),
    stderr_snippet: (if ($stderr_snippet | length) > 0 then $stderr_snippet else "" end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg target_file "${target_file}" \
  --arg target_dir "${target_dir}" \
  --arg write_mode "append" \
  --argjson attempt_bytes "${attempt_bytes}" \
  '{target_file: $target_file,
    target_dir: $target_dir,
    write_mode: $write_mode,
    attempt_bytes: $attempt_bytes}')

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
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
