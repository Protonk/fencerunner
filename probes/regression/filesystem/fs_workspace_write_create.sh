#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_workspace_write_create"
primary_capability_id="cap_fs_write_workspace_tree"
attempt_line="codex-fence workspace write $(date -u +%Y-%m-%dT%H:%M:%SZ) $$"
target_path=$(mktemp "${repo_root}/.codex-fence-workspace-write.XXXXXX")
printf -v command_executed "printf %q > %q" "${attempt_line}" "${target_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}" "${target_path}"' EXIT

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

raw_payload=$(jq -n \
  --arg target_path "${target_path}" \
  --arg relative_path "${relative_path}" \
  --arg written_contents "${file_contents}" \
  --argjson written_bytes "${written_bytes}" \
  '{target_path: $target_path,
    relative_path: (if ($relative_path | length) > 0 then $relative_path else null end),
    written_bytes: $written_bytes,
    written_contents: $written_contents}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg write_mode "truncate" \
  --arg target_path "${target_path}" \
  --arg relative_path "${relative_path}" \
  --argjson bytes "${written_bytes}" \
  '{write_mode: $write_mode,
    bytes: $bytes,
    target_path: $target_path,
    relative_path: (if ($relative_path | length) > 0 then $relative_path else null end)}')

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
