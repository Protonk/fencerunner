#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

# Prefer local build artifacts when present so developers can iterate without syncing bin/.
paging_stress_bin="${repo_root}/bin/paging-stress"
if [[ -x "${repo_root}/target/debug/paging-stress" ]]; then
  paging_stress_bin="${repo_root}/target/debug/paging-stress"
elif [[ -x "${repo_root}/target/release/paging-stress" ]]; then
  paging_stress_bin="${repo_root}/target/release/paging-stress"
fi

json_extract_bin="${repo_root}/bin/json-extract"
if [[ -x "${repo_root}/target/debug/json-extract" ]]; then
  json_extract_bin="${repo_root}/target/debug/json-extract"
elif [[ -x "${repo_root}/target/release/json-extract" ]]; then
  json_extract_bin="${repo_root}/target/release/json-extract"
fi

probe_name="proc_paging_stress"
primary_capability_id="cap_proc_fork_and_child_spawn"

megabytes=4
passes=2
pattern="random"
helper_max_seconds=2
wrapper_timeout_seconds=4

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
status_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}" "${status_tmp}"' EXIT

python_available="1"
if ! command -v python3 >/dev/null 2>&1; then
  echo "proc_paging_stress: python3 not found" >&2
  python_available="0"
fi

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

status="error"
errno_value=""
message="paging-stress did not run"
raw_exit_code=""
helper_timeout="false"
helper_exit_code="null"
helper_error=""
status_reason=""

if [[ ! -x "${paging_stress_bin}" ]]; then
  status_reason="paging-stress helper missing at ${paging_stress_bin}"
elif [[ "${python_available}" != "1" ]]; then
  status_reason="python3 unavailable to wrap paging-stress"
else
  # Use python3 for a portable timeout wrapper that keeps helper output off stdout.
  python3 - "$wrapper_timeout_seconds" "$stdout_tmp" "$stderr_tmp" "$status_tmp" "${paging_stress_cmd[@]}" <<'PY'
import json
import subprocess
import sys

timeout = float(sys.argv[1])
stdout_path = sys.argv[2]
stderr_path = sys.argv[3]
status_path = sys.argv[4]
cmd = sys.argv[5:]

status = {"timeout": False, "returncode": None, "error": None}
try:
    completed = subprocess.run(cmd, capture_output=True, timeout=timeout)
    status["returncode"] = completed.returncode
    with open(stdout_path, "wb") as fh:
        fh.write(completed.stdout)
    with open(stderr_path, "wb") as fh:
        fh.write(completed.stderr)
except subprocess.TimeoutExpired as exc:
    status["timeout"] = True
    with open(stdout_path, "wb") as fh:
        fh.write(exc.stdout or b"")
    with open(stderr_path, "wb") as fh:
        fh.write(exc.stderr or b"")
except Exception as exc:  # noqa: BLE001
    status["error"] = str(exc)
    with open(stdout_path, "wb") as fh:
        fh.write(b"")
    with open(stderr_path, "wb") as fh:
        fh.write(str(exc).encode("utf-8", errors="replace"))

with open(status_path, "w", encoding="utf-8") as fh:
    json.dump(status, fh)
PY
fi

if [[ -n "${status_reason}" ]]; then
  helper_error="${status_reason}"
else
  if [[ ! -x "${json_extract_bin}" ]]; then
    helper_timeout="false"
    helper_exit_code="null"
    helper_error="json-extract missing at ${json_extract_bin}"
  elif [[ -s "${status_tmp}" ]]; then
    helper_timeout=$("${json_extract_bin}" --file "${status_tmp}" --pointer "/timeout" --type bool --default "false")
    helper_exit_code=$("${json_extract_bin}" --file "${status_tmp}" --pointer "/returncode" --default "null")
    helper_error_json=$("${json_extract_bin}" --file "${status_tmp}" --pointer "/error" --default "\"\"")
    helper_error=${helper_error_json#\"}
    helper_error=${helper_error%\"}
  else
    helper_timeout="false"
    helper_exit_code="null"
    helper_error="missing status"
  fi
fi

if [[ "${helper_exit_code}" != "null" ]]; then
  raw_exit_code="${helper_exit_code}"
fi

stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

if [[ "${helper_timeout}" == "true" ]]; then
  status="partial"
  message="paging-stress wrapper timeout after ${wrapper_timeout_seconds}s"
  raw_exit_code="${raw_exit_code:-124}"
elif [[ "${raw_exit_code}" == "0" ]]; then
  status="success"
  message="paging-stress completed"
elif [[ "${raw_exit_code}" == "3" ]]; then
  status="partial"
  message="paging-stress self-timed out"
elif [[ "${raw_exit_code}" == "1" ]]; then
  status="error"
  message="paging-stress rejected arguments"
elif [[ "${raw_exit_code}" == "2" ]]; then
  status="error"
  message="paging-stress reported internal error"
elif [[ -n "${helper_error}" ]]; then
  status="error"
  message="paging-stress failed: ${helper_error}"
else
  status="error"
  message="paging-stress exited with ${raw_exit_code:-unknown code}"
fi

operation_args=$(cat <<EOF
{"megabytes":${megabytes},"passes":${passes},"pattern":"${pattern}","helper_max_seconds":${helper_max_seconds},"wrapper_timeout_seconds":${wrapper_timeout_seconds}}
EOF
)

"${emit_record_bin}" \
  --probe-name "${probe_name}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --operation-kind "proc.exec" \
  --target "${paging_stress_bin}" \
  --outcome "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field-json "helper_timeout" "${helper_timeout}" \
  --payload-raw-field-json "helper_exit_code" "${helper_exit_code}" \
  --payload-raw-field "helper_error" "${helper_error}" \
  --operation-args "${operation_args}"
