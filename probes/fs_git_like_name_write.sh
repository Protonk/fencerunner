#!/usr/bin/env bash
set -euo pipefail

# Experiment: write inside a directory whose name closely resembles .git but is not
# an exact match (.git-shadow) to see whether write guards over-match.
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
probe_name="fs_git_like_name_write"
probe_version="1"
primary_capability_id="cap_fs_read_git_metadata"

probe_tmp_root="${repo_root}/tmp/fs_git_like_name_write"
mkdir -p "${probe_tmp_root}" 2>/dev/null || true
unique_suffix="$(date -u +%s%N).$$"

safe_tmp_file() {
  local label="$1"
  local candidates=()
  if [[ -d "${probe_tmp_root}" ]]; then
    candidates+=("${probe_tmp_root}")
  fi
  if [[ -n "${TMPDIR:-}" ]]; then
    candidates+=("${TMPDIR%/}")
  fi
  candidates+=("/tmp")

  local dir path
  for dir in "${candidates[@]}"; do
    if [[ -z "${dir}" ]]; then
      continue
    fi
    if mkdir -p "${dir}" 2>/dev/null; then
      if path=$(TMPDIR="${dir}" mktemp "${label}.${unique_suffix}.XXXXXX" 2>/dev/null); then
        printf '%s\n' "${path}"
        return 0
      fi
    fi
  done
  printf ''
  return 1
}

workspace_fake_root_template="${probe_tmp_root}/workspace.XXXXXX"
workspace_fake_root=""
workspace_setup_ok="true"
mktemp_exit_code=0

project_segment="project.git-like"
git_like_component=".git-shadow"

timestamp=$(date -u +"%Y%m%dT%H%M%SZ")
attempt_line="git-like name write ${timestamp}"
attempt_bytes=$(( ${#attempt_line} + 1 ))

stdout_tmp=$(safe_tmp_file "stdout")
stderr_tmp=$(safe_tmp_file "stderr")
mktemp_error_tmp=$(safe_tmp_file "mktemp_error")
cleanup() {
  for candidate in "${stdout_tmp}" "${stderr_tmp}" "${mktemp_error_tmp}"; do
    if [[ -n "${candidate}" && -f "${candidate}" ]]; then
      rm -f "${candidate}" || true
    fi
  done
  if [[ -n "${workspace_fake_root}" ]]; then
    rm -rf "${workspace_fake_root}"
  fi
}
trap cleanup EXIT

if workspace_fake_root=$(mktemp -d "${workspace_fake_root_template}" 2>"${mktemp_error_tmp:-/dev/null}"); then
  workspace_setup_ok="true"
else
  workspace_setup_ok="false"
  mktemp_exit_code=$?
fi

target_root="${workspace_fake_root:-${workspace_fake_root_template}}"
target_dir="${target_root}/${project_segment}/${git_like_component}/objects"
target_file="${target_dir}/write_probe.txt"

status="error"
errno_value=""
message=""
raw_exit_code=""
stdout_text=""
stderr_text=""
command_executed="(not attempted)"

if [[ "${workspace_setup_ok}" == "true" ]]; then
  printf -v command_executed "mkdir -p %q" "${target_dir}"
  set +e
  mkdir -p "${target_dir}" 2>"${stderr_tmp:-/dev/null}"
  mkdir_status=$?
  set -e

  if [[ ${mkdir_status} -ne 0 ]]; then
    raw_exit_code="${mkdir_status}"
    if [[ -n "${stderr_tmp}" && -f "${stderr_tmp}" ]]; then
      stderr_text=$(tr -d '\0' <"${stderr_tmp}" 2>/dev/null || true)
    fi
    lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
    if [[ "${lower_err}" == *"permission denied"* ]]; then
      status="denied"
      errno_value="EACCES"
      message="Permission denied creating git-like directory"
    elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
      status="denied"
      errno_value="EPERM"
      message="Operation not permitted creating git-like directory"
    else
      status="error"
      message="Failed to create git-like directory"
    fi
  else
    printf -v command_executed "printf %q >> %q" "${attempt_line}" "${target_file}"

    set +e
    {
      printf '%s\n' "${attempt_line}" >>"${target_file}"
    } >"${stdout_tmp:-/dev/null}" 2>"${stderr_tmp:-/dev/null}"
    exit_code=$?
    set -e
    raw_exit_code="${exit_code}"
    if [[ -n "${stdout_tmp}" && -f "${stdout_tmp}" ]]; then
      stdout_text=$(tr -d '\0' <"${stdout_tmp}" 2>/dev/null || true)
    fi
    if [[ -n "${stderr_tmp}" && -f "${stderr_tmp}" ]]; then
      stderr_text=$(tr -d '\0' <"${stderr_tmp}" 2>/dev/null || true)
    fi

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
  fi
else
  command_executed=$(printf "mktemp -d %q" "${workspace_fake_root_template}")
  raw_exit_code="${mktemp_exit_code}"
  if [[ -n "${mktemp_error_tmp}" && -f "${mktemp_error_tmp}" ]]; then
    stderr_text=$(tr -d '\0' <"${mktemp_error_tmp}" 2>/dev/null || true)
  fi
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied creating git-like workspace"
  elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Operation not permitted creating git-like workspace"
  else
    status="error"
    message="Failed to create git-like workspace"
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

workspace_created_json="false"
if [[ "${workspace_setup_ok}" == "true" ]]; then
  workspace_created_json="true"
fi
workspace_error_text=""
if [[ "${workspace_setup_ok}" != "true" ]]; then
  workspace_error_text="${stderr_text}"
fi

emit_args=(
  --run-mode "${run_mode}"
  --probe-name "${probe_name}"
  --probe-version "${probe_version}"
  --primary-capability-id "${primary_capability_id}"
  --command "${command_executed}"
  --category "fs"
  --verb "write"
  --target "${target_file}"
  --status "${status}"
  --errno "${errno_value}"
  --message "${message}"
  --raw-exit-code "${raw_exit_code}"
  --payload-stdout "${stdout_text}"
  --payload-stderr "${stderr_text}"
  --payload-raw-field "target_file" "${target_file}"
  --payload-raw-field "target_realpath" "${target_realpath}"
  --payload-raw-field "git_like_component" "${git_like_component}"
  --payload-raw-field "project_segment" "${project_segment}"
  --payload-raw-field "workspace_root_template" "${workspace_fake_root_template}"
  --payload-raw-field-json "workspace_created" "${workspace_created_json}"
  --operation-arg "target_path" "${target_file}"
  --operation-arg "git_like_component" "${git_like_component}"
  --operation-arg "sibling_project_segment" "${project_segment}"
  --operation-arg-json "attempt_bytes" "${attempt_bytes}"
  --operation-arg "write_mode" "append"
  --operation-arg-json "looks_like_git" "true"
  --operation-arg-json "workspace_created" "${workspace_created_json}"
)

if [[ -n "${git_dir_realpath}" ]]; then
  emit_args+=(--payload-raw-field "git_dir_realpath" "${git_dir_realpath}")
else
  emit_args+=(--payload-raw-null "git_dir_realpath")
fi

if [[ -n "${target_size}" ]]; then
  emit_args+=(--payload-raw-field-json "resulting_size" "${target_size}")
else
  emit_args+=(--payload-raw-null "resulting_size")
fi

if [[ "${workspace_created_json}" == "true" ]]; then
  emit_args+=(--payload-raw-field "workspace_root" "${workspace_fake_root}")
else
  emit_args+=(--payload-raw-null "workspace_root")
fi

if [[ -n "${workspace_error_text}" ]]; then
  emit_args+=(--payload-raw-field "workspace_error" "${workspace_error_text}")
else
  emit_args+=(--payload-raw-null "workspace_error")
fi

"${emit_record_bin}" "${emit_args[@]}"
