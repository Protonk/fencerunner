#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="sandbox_debug_log_capture"
primary_capability_id="cap_sandbox_debug_and_trace_logging"
log_window="${FENCE_SANDBOX_LOG_WINDOW:-30s}"
log_predicate='eventMessage CONTAINS "Sandbox:"'
printf -v command_executed "log show --last %s --style compact --predicate %q" "${log_window}" "${log_predicate}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trigger_err_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${trigger_err_tmp}"' EXIT

trigger_exit_code=""
trigger_stderr=""

if [[ "${CODEX_SANDBOX:-}" != "seatbelt" ]]; then
  set +e
  codex sandbox macos --full-auto -- bash -lc '/bin/ps >/dev/null' >/dev/null 2>"${trigger_err_tmp}"
  trigger_status=$?
  set -e
  trigger_exit_code="${trigger_status}"
  trigger_stderr=$(tr -d '\0' <"${trigger_err_tmp}")
else
  trigger_exit_code="skipped"
  trigger_stderr="skipped (already sandboxed)"
fi

status="error"
errno_value=""
message=""
raw_exit_code=""
sandbox_line_count=0

set +e
log show --last "${log_window}" --style compact --predicate "${log_predicate}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e
raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

if [[ -n "${stdout_text}" ]]; then
  sandbox_line_count=$(printf '%s' "${stdout_text}" | grep -c 'Sandbox:' || true)
fi

if [[ ${exit_code} -eq 0 ]]; then
  if [[ ${sandbox_line_count} -gt 0 ]]; then
    status="success"
    message="Captured ${sandbox_line_count} sandbox log entries"
  else
    status="partial"
    message="No sandbox log entries captured"
  fi
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"cannot run while sandboxed"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="log tool blocked inside sandbox"
  else
    status="error"
    message="log show failed with exit code ${exit_code}"
  fi
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "sandbox_meta" \
  --verb "inspect" \
  --target "log show" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "log_window" "${log_window}" \
  --payload-raw-field "log_predicate" "${log_predicate}" \
  --payload-raw-field "trigger_exit_code" "${trigger_exit_code}" \
  --payload-raw-field "trigger_stderr" "${trigger_stderr}" \
  --payload-raw-field-json "sandbox_line_count" "${sandbox_line_count}" \
  --operation-arg "log_window" "${log_window}" \
  --operation-arg "log_predicate" "${log_predicate}"
