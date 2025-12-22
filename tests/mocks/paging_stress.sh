#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Fixture probe that exercises the paging-stress helper and emits a deterministic
# boundary object for helper/probe-exec tests.
# -----------------------------------------------------------------------------
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
repo_root_candidate="${script_dir}"
repo_root=""
while [[ -z "${repo_root}" && "${repo_root_candidate}" != "/" ]]; do
  if [[ -x "${repo_root_candidate}/bin/emit-record" ]]; then
    repo_root="${repo_root_candidate}"
    break
  fi
  repo_root_candidate=$(cd "${repo_root_candidate}/.." >/dev/null 2>&1 && pwd)
done
if [[ -z "${repo_root}" ]]; then
  echo "paging_stress_fixture: unable to locate repo root" >&2
  exit 1
fi

emit_record_bin="${repo_root}/bin/emit-record"
target_debug="${repo_root}/target/debug/emit-record"
target_release="${repo_root}/target/release/emit-record"
if [[ -x "${target_debug}" ]]; then
  emit_record_bin="${target_debug}"
elif [[ -x "${target_release}" ]]; then
  emit_record_bin="${target_release}"
fi

paging_stress_bin="${repo_root}/bin/paging-stress"
target_debug="${repo_root}/target/debug/paging-stress"
target_release="${repo_root}/target/release/paging-stress"
if [[ -x "${target_debug}" ]]; then
  paging_stress_bin="${target_debug}"
elif [[ -x "${target_release}" ]]; then
  paging_stress_bin="${target_release}"
fi

probe_name="tests_paging_stress"
primary_capability_id="cap_proc_fork_and_child_spawn"

megabytes=1
passes=1
pattern="random"
helper_max_seconds=2

paging_stress_cmd=(
  "${paging_stress_bin}"
  "--megabytes" "${megabytes}"
  "--passes" "${passes}"
  "--pattern" "${pattern}"
  "--max-seconds" "${helper_max_seconds}"
)

printf -v command_executed "%q" "${paging_stress_cmd[0]}"
for ((i = 1; i < ${#paging_stress_cmd[@]}; i++)); do
  printf -v command_executed "%s %q" "${command_executed}" "${paging_stress_cmd[i]}"
done

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

outcome="error"
message=""
exit_code="127"
helper_timeout="false"
helper_exit_code="127"
helper_error=""

if [[ -x "${paging_stress_bin}" ]]; then
  exit_code=0
  if ! "${paging_stress_cmd[@]}" >"${stdout_tmp}" 2>"${stderr_tmp}"; then
    exit_code=$?
  fi
  helper_exit_code="${exit_code}"
  if [[ "${exit_code}" -eq 0 ]]; then
    outcome="success"
    message="paging-stress completed"
  else
    outcome="error"
    message="paging-stress exited with ${exit_code}"
    helper_error="${message}"
  fi
else
  outcome="error"
  message="paging-stress helper missing at ${paging_stress_bin}"
  helper_error="${message}"
fi

stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

operation_args=$(cat <<EOF
{"megabytes":${megabytes},"passes":${passes},"pattern":"${pattern}","helper_max_seconds":${helper_max_seconds}}
EOF
)

"${emit_record_bin}" \
  --probe-name "${probe_name}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --operation-kind "proc.exec" \
  --target "${paging_stress_bin}" \
  --outcome "${outcome}" \
  --message "${message}" \
  --exit-code "${exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field-json "helper_timeout" "${helper_timeout}" \
  --payload-raw-field-json "helper_exit_code" "${helper_exit_code}" \
  --payload-raw-field "helper_error" "${helper_error}" \
  --operation-args "${operation_args}"
