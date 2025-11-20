#!/usr/bin/env bash
set -euo pipefail

# cap_fs_read_workspace_tree denial case: attempt to read ~/Documents using ../ segments that escape the workspace.
probe_name="fs_workspace_relative_escape_read_guard"
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
portable_path_helper="${repo_root}/bin/portable-path"
if [[ ! -x "${portable_path_helper}" ]]; then
  echo "probe ${probe_name:-fs_workspace_relative_escape_read_guard}: missing helper ${portable_path_helper}. Build it with 'cargo build --release'." >&2
  exit 1
fi

portable_relpath() {
  "${portable_path_helper}" relpath "$1" "$2"
}

portable_realpath() {
  "${portable_path_helper}" realpath "$1"
}

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_version="1"
primary_capability_id="cap_fs_read_workspace_tree"

relative_home=$(portable_relpath "${HOME}" "${repo_root}")
if [[ -z "${relative_home}" || "${relative_home}" == /* ]]; then
  echo "probe ${probe_name}: unable to compute relative path from workspace to home" >&2
  exit 1
fi
escape_target="${repo_root}/${relative_home}/Documents"
canonical_target=$(portable_realpath "${escape_target}")
if [[ -z "${canonical_target}" ]]; then
  canonical_target="${escape_target}"
fi
printf -v command_executed "ls -ld %q" "${escape_target}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

documents_dir="${HOME}/Documents"

if [[ ! -d "${documents_dir}" ]]; then
  status="error"
  errno_value="ENOENT"
  message="~/Documents not found for escape attempt"
  raw_exit_code="1"
  stdout_text=""
  stderr_text="Documents directory missing at ${documents_dir}"
else
  set +e
  ls -ld "${escape_target}" >"${stdout_tmp}" 2>"${stderr_tmp}"
  exit_code=$?
  set -e

  raw_exit_code="${exit_code}"
  stdout_text=$(tr -d '\0' <"${stdout_tmp}")
  stderr_text=$(tr -d '\0' <"${stderr_tmp}")

  if [[ ${exit_code} -eq 0 ]]; then
    status="success"
    message="Listed ~/Documents via workspace-relative escape path"
  else
    lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
    if [[ "${lower_err}" == *"permission denied"* ]]; then
      status="denied"
      errno_value="EACCES"
      message="Permission denied escaping workspace to ~/Documents"
    elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
      status="denied"
      errno_value="EPERM"
      message="Operation not permitted escaping workspace to ~/Documents"
    elif [[ "${lower_err}" == *"no such file"* ]]; then
      status="error"
      errno_value="ENOENT"
      message="System reported ~/Documents missing"
    else
      status="error"
      errno_value=""
      message="ls -ld escape attempt failed with exit code ${exit_code}"
    fi
  fi
fi

raw_json=$(jq -n \
  --arg relative_path "${escape_target}" \
  --arg canonical_path "${canonical_target}" \
  --arg relative_home "${relative_home}" \
  --arg documents_dir "${documents_dir}" \
  '{relative_escape_path: $relative_path,
    canonical_target: $canonical_path,
    relative_home_path: $relative_home,
    documents_dir: $documents_dir,
    via_relative_segments: true}')

jq -n \
  --arg stdout_snippet "${stdout_text:-}" \
  --arg stderr_snippet "${stderr_text:-}" \
  --argjson raw "${raw_json}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg path_type "directory" \
  --arg escape_strategy "dotdot_segments" \
  --arg relative_home "${relative_home}" \
  '{path_type: $path_type,
    escape_strategy: $escape_strategy,
    relative_home_path: $relative_home}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "read" \
  --target "${escape_target}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
