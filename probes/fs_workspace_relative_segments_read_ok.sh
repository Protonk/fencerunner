#!/usr/bin/env bash
set -euo pipefail

# cap_fs_read_workspace_tree success case: read a workspace file using ./ and ../ segments.
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
probe_name="fs_workspace_relative_segments_read_ok"
probe_version="1"
primary_capability_id="cap_fs_read_workspace_tree"
lines_to_read="${FENCE_FS_WORKSPACE_REL_SEGMENTS_LINES:-8}"
if ! [[ "${lines_to_read}" =~ ^[0-9]+$ ]]; then
  lines_to_read=8
fi
relative_target="${repo_root}/./docs/../README.md"
canonical_target=$(portable_realpath "${relative_target}")
if [[ -z "${canonical_target}" ]]; then
  canonical_target="${relative_target}"
fi
printf -v command_executed "head -n %q %q" "${lines_to_read}" "${relative_target}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
head -n "${lines_to_read}" "${relative_target}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Read ${lines_to_read} lines via relative workspace path"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied when reading workspace file"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted when reading workspace file"
  elif [[ "${lower_err}" == *"no such file"* ]]; then
    status="error"
    errno_value="ENOENT"
    message="Workspace README missing"
  else
    status="error"
    errno_value=""
    message="Relative head read failed with exit code ${exit_code}"
  fi
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "read" \
  --target "${relative_target}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "relative_path" "${relative_target}" \
  --payload-raw-field "canonical_path" "${canonical_target}" \
  --payload-raw-field-json "via_relative_segments" "true" \
  --payload-raw-field-json "lines_requested" "${lines_to_read}" \
  --payload-raw-field-json "stdout_length" "${#stdout_text}" \
  --payload-raw-field-json "stderr_length" "${#stderr_text}" \
  --operation-arg "read_mode" "head" \
  --operation-arg-json "lines" "${lines_to_read}" \
  --operation-arg-json "via_relative_segments" "true"
