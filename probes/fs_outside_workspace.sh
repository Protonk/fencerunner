#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_outside_workspace"
target_path="${FENCE_FS_OUTSIDE_TARGET:-/tmp/codex-fence-outside-root-test}"
attempt_line="codex-fence write $(date -u +%Y-%m-%dT%H:%M:%SZ)"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""

set +e
bash -c 'printf "%s\n" "$1" >> "$2"' _ "${attempt_line}" "${target_path}" \
  >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

stderr_text=$(tr -d '\0' <"${stderr_tmp}")
stdout_text=$(tr -d '\0' <"${stdout_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="allowed"
  message="Write succeeded"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted"
  elif [[ "${lower_err}" == *"read-only file system"* ]]; then
    status="denied"
    errno_value="EROFS"
    message="Read-only file system"
  else
    status="error"
    message="Write failed with exit code ${exit_code}"
    errno_value=""
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
  --argjson attempt_bytes "${#attempt_line}" \
  '{write_mode: $write_mode, attempt_bytes: $attempt_bytes}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --category "fs" \
  --verb "write" \
  --target "${target_path}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
