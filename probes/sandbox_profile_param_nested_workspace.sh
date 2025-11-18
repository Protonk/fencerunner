#!/usr/bin/env bash
set -euo pipefail

# Probe for cap_sandbox_profile_parameterization: run a nested codex sandbox in a
# throwaway workspace outside the repo and confirm it can specialize the profile
# to that path by writing a marker file there.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="sandbox_profile_param_nested_workspace"
probe_version="1"
primary_capability_id="cap_sandbox_profile_parameterization"
secondary_capability_id="cap_fs_write_workspace_tree"

nested_workspace=$(mktemp -d "/tmp/codex-fence-param-ws.XXXXXX")
inner_script="${nested_workspace}/nested_write.sh"
inner_script_name=$(basename "${inner_script}")
marker_path="${nested_workspace}/param_write_marker.txt"
attempt_line="nested sandbox param $(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf -v command_executed "(cd %q && codex sandbox macos --full-auto -- ./%s %q)" \
  "${nested_workspace}" "${inner_script_name}" "${attempt_line}"

cat <<'EOS' >"${inner_script}"
#!/usr/bin/env bash
set -euo pipefail
message="${1:-nested sandbox run}"
printf '%s\n' "${message}" > param_write_marker.txt
EOS
chmod +x "${inner_script}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
payload_tmp=$(mktemp)
cleanup() {
  rm -f "${stdout_tmp}" "${stderr_tmp}" "${payload_tmp}"
  rm -rf "${nested_workspace}"
}
trap cleanup EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
(
  cd "${nested_workspace}"
  codex sandbox macos --full-auto -- "./${inner_script_name}" "${attempt_line}"
) >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

marker_exists_json="false"
marker_contents=""
if [[ -f "${marker_path}" ]]; then
  marker_exists_json="true"
  marker_contents=$(tr -d '\0' <"${marker_path}")
fi

lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
if [[ ${exit_code} -eq 0 && "${marker_exists_json}" == "true" ]]; then
  status="success"
  message="Nested sandbox wrote marker in its workspace"
elif [[ ${exit_code} -eq 0 ]]; then
  status="partial"
  message="Nested sandbox exited 0 but marker missing"
elif [[ "${lower_err}" == *"operation not permitted"* ]]; then
  status="denied"
  errno_value="EPERM"
  message="Nested sandbox denied: operation not permitted"
elif [[ "${lower_err}" == *"permission denied"* ]]; then
  status="denied"
  errno_value="EACCES"
  message="Nested sandbox denied: permission denied"
else
  status="error"
  message="Nested sandbox failed with exit code ${exit_code}"
fi

raw_payload=$(jq -n \
  --arg nested_workspace "${nested_workspace}" \
  --arg marker_path "${marker_path}" \
  --arg inner_script "${inner_script_name}" \
  --arg attempt_line "${attempt_line}" \
  --arg marker_contents "${marker_contents}" \
  --arg stdout "${stdout_text}" \
  --arg stderr "${stderr_text}" \
  --argjson marker_exists "${marker_exists_json}" \
  --argjson nested_exit "${exit_code}" \
  '{nested_workspace: $nested_workspace,
    marker_path: $marker_path,
    inner_script: $inner_script,
    attempt_line: $attempt_line,
    marker_exists: $marker_exists,
    marker_contents: ($marker_contents | if length > 400 then (.[:400] + "…") else . end),
    nested_exit_code: $nested_exit,
    nested_stdout: ($stdout | if length > 400 then (.[:400] + "…") else . end),
    nested_stderr: ($stderr | if length > 400 then (.[:400] + "…") else . end)}')

jq -n \
  --arg stdout_snippet "${stdout_text}" \
  --arg stderr_snippet "${stderr_text}" \
  --argjson raw "${raw_payload}" \
  '{stdout_snippet: ($stdout_snippet | if length > 400 then (.[:400] + "…") else . end),
    stderr_snippet: ($stderr_snippet | if length > 400 then (.[:400] + "…") else . end),
    raw: $raw}' >"${payload_tmp}"

operation_args=$(jq -n \
  --arg nested_workspace "${nested_workspace}" \
  --arg marker_path "${marker_path}" \
  --arg attempt_line "${attempt_line}" \
  '{nested_workspace: $nested_workspace,
    marker_path: $marker_path,
    attempt_line: $attempt_line}')

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --secondary-capability-id "${secondary_capability_id}" \
  --command "${command_executed}" \
  --category "sandbox_meta" \
  --verb "parameterize" \
  --target "${nested_workspace}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
