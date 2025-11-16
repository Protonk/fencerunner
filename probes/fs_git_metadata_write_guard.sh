#!/usr/bin/env bash
set -euo pipefail

# Targets cap_fs_read_git_metadata: verifies writes inside .git are blocked even though reads remain allowed.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_git_metadata_write_guard"
primary_capability_id="cap_fs_read_git_metadata"
target_path="${repo_root}/.git/codex-fence-write-test"
attempt_line="codex-fence git-metadata-write $(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf -v command_executed "printf %q >> %q" "${attempt_line}" "${target_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
remove_target_on_exit="false"
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"
  if [[ "${remove_target_on_exit}" == "true" && -f "${target_path}" ]]; then
    rm -f "${target_path}"
  fi
}
trap cleanup EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
bash -c 'printf "%s\n" "$1" >> "$2"' _ "${attempt_line}" "${target_path}" \
  >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
stdout_text=$(tr -d '\0' <"${stdout_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Write inside .git succeeded"
  remove_target_on_exit="true"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied writing .git"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted"
  elif [[ "${lower_err}" == *"read-only file system"* ]]; then
    status="denied"
    errno_value="EROFS"
    message="Read-only file system"
  elif [[ "${lower_err}" == *"no such file or directory"* ]]; then
    status="error"
    errno_value="ENOENT"
    message=".git directory not found"
  else
    status="error"
    message="Write failed with exit code ${exit_code}"
  fi
fi

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw '{}' \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg write_mode "append" \
  --arg path_context ".git" \
  --argjson attempt_bytes "${#attempt_line}" \
  '{write_mode: $write_mode, path_context: $path_context, attempt_bytes: $attempt_bytes}')

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
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
