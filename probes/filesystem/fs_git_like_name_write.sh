#!/usr/bin/env bash
set -euo pipefail

# Experiment: write inside a directory whose name closely resembles .git but is not
# an exact match (.git-shadow) to see whether write guards over-match.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
helpers_lib="${repo_root}/tools/lib/helpers.sh"
# shellcheck source=tools/lib/helpers.sh
source "${helpers_lib}"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="fs_git_like_name_write"
probe_version="1"
primary_capability_id="cap_fs_read_git_metadata"

workspace_fake_root=$(mktemp -d "${repo_root}/tmp_git_like_name.XXXXXX")
project_segment="project.git-like"
git_like_component=".git-shadow"
target_dir="${workspace_fake_root}/${project_segment}/${git_like_component}/objects"
mkdir -p "${target_dir}"
target_file="${target_dir}/write_probe.txt"

timestamp=$(date -u +"%Y%m%dT%H%M%SZ")
attempt_line="git-like name write ${timestamp}"
attempt_bytes=$(( ${#attempt_line} + 1 ))
printf -v command_executed "printf %q >> %q" "${attempt_line}" "${target_file}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"
  rm -rf "${workspace_fake_root}"
}
trap cleanup EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
{
  printf '%s\n' "${attempt_line}" >>"${target_file}"
} >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Write inside git-like directory succeeded"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied writing inside git-like directory"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted inside git-like directory"
  elif [[ "${lower_err}" == *"no such file or directory"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="Git-like directory missing"
  else
    status="error"
    message="Git-like write failed with exit code ${exit_code}"
  fi
fi

truncate_field() {
  local value="$1"
  if [[ ${#value} -gt 400 ]]; then
    printf '%s…' "${value:0:400}"
  else
    printf '%s' "${value}"
  fi
}

stdout_snippet=$(truncate_field "${stdout_text}")
stderr_snippet=$(truncate_field "${stderr_text}")

target_realpath=$(portable_realpath "${target_file}")

git_dir_path=$(cd "${repo_root}" && git rev-parse --git-dir 2>/dev/null || true)
if [[ -n "${git_dir_path}" ]]; then
  git_dir_realpath=$(cd "${repo_root}" && portable_realpath "${git_dir_path}")
else
  git_dir_realpath=""
fi

target_size=""
if [[ -f "${target_file}" ]]; then
  target_size=$(wc -c <"${target_file}" | tr -d '[:space:]')
fi

raw_payload=$(jq -n \
  --arg target_file "${target_file}" \
  --arg target_realpath "${target_realpath}" \
  --arg git_like_component "${git_like_component}" \
  --arg project_segment "${project_segment}" \
  --arg git_dir "${git_dir_realpath}" \
  --arg target_size "${target_size}" \
  '{
    target_file: $target_file,
    target_realpath: $target_realpath,
    git_like_component: $git_like_component,
    project_segment: $project_segment,
    git_dir_realpath: (if ($git_dir | length) > 0 then $git_dir else null end),
    resulting_size: (if ($target_size | length) > 0 then ($target_size | tonumber) else null end)
  }')

jq -n \
  --arg stdout_snippet "${stdout_snippet}" \
  --arg stderr_snippet "${stderr_snippet}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: (if ($stdout_snippet | length) > 0 then $stdout_snippet else "" end),
    stderr_snippet: (if ($stderr_snippet | length) > 0 then $stderr_snippet else "" end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg target_path "${target_file}" \
  --arg git_like_component "${git_like_component}" \
  --arg project_segment "${project_segment}" \
  --argjson attempt_bytes "${attempt_bytes}" \
  '{
    target_path: $target_path,
    git_like_component: $git_like_component,
    sibling_project_segment: $project_segment,
    attempt_bytes: $attempt_bytes,
    write_mode: "append",
    looks_like_git: true
  }')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
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
