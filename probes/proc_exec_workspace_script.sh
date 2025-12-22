#!/usr/bin/env bash
set -euo pipefail

# why: executing a freshly created workspace script checks if sandbox allows running new files, not just system toolchains
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

probe_name="proc_exec_workspace_script"
primary_capability_id="cap_proc_fork_and_child_spawn"
secondary_capability_id="cap_fs_write_workspace_tree"

json_key() {
  local key="$1"
  if [[ -n "${PROBE_CONTRACT_GATE_STATE_DIR:-}" ]]; then
    printf '"%s"' "${key}"
  else
    printf '%s' "${key}"
  fi
}

work_dir=$(mktemp -d "${repo_root}/tmp/workspace_exec.XXXXXX")
script_path="${work_dir}/workspace_exec_test.sh"
expected_line="workspace script executed"

cat <<'EOS' >"${script_path}"
#!/usr/bin/env bash
set -euo pipefail
echo "workspace script executed"
EOS
chmod +x "${script_path}"

printf -v command_executed "%q" "${script_path}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}"
  rm -rf "${work_dir}"
}
trap cleanup EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
"${script_path}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")
lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

stdout_trimmed=$(printf '%s' "${stdout_text}" | head -n 5)
matched="false"
if [[ "${stdout_text}" == *"${expected_line}"* ]]; then
  matched="true"
fi

if [[ ${exit_code} -eq 0 && "${matched}" == "true" ]]; then
  status="success"
  message="Workspace script executed directly"
elif [[ ${exit_code} -eq 0 ]]; then
  status="partial"
  message="Script ran but output mismatch"
elif [[ "${lower_err}" == *"permission denied"* ]]; then
  status="denied"
  errno_value="EACCES"
  message="Execution denied for workspace script"
elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
  status="denied"
  errno_value="EPERM"
  message="Operation not permitted executing workspace script"
else
  status="error"
  message="Workspace script failed with exit code ${exit_code}"
fi

"${emit_record_bin}" \
  --probe-name "${probe_name}" \
  --primary-capability-id "${primary_capability_id}" \
  --secondary-capability-id "${secondary_capability_id}" \
  --command "${command_executed}" \
  --operation-kind "proc.exec" \
  --target "${script_path}" \
  --outcome "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "script_path" "${script_path}" \
  --payload-raw-field "work_dir" "${work_dir}" \
  --payload-raw-field "expected_line" "${expected_line}" \
  --payload-raw-field "stdout_snippet" "${stdout_trimmed}" \
  --payload-raw-field-json "$(json_key "matched_expected_output")" "${matched}" \
  --operation-arg "script_path" "${script_path}" \
  --operation-arg "invocation" "direct_exec" \
  --operation-arg "expected_line" "${expected_line}"
