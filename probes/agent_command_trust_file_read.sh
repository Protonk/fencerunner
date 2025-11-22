#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="agent_command_trust_file_read"
primary_capability_id="cap_agent_command_trust_list"
target_path="${FENCE_TRUST_LIST_PATH:-${HOME}/.config/codex/trust-list.json}"

printf -v command_executed "cat %q" "${target_path}"

scratch_dir="${repo_root}/tmp/${probe_name}"
if ! mkdir -p "${scratch_dir}" 2>/dev/null; then
  scratch_dir=""
fi

payload_tmp=""
cleanup() {
  if [[ -n "${payload_tmp}" && -f "${payload_tmp}" ]]; then
    rm -f "${payload_tmp}"
  fi
}
trap cleanup EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
stderr_text=$(cat "${target_path}" 2>&1 >/dev/null)
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stderr_text=$(printf '%s' "${stderr_text}" | tr -d '\0')
lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Read trust list file"
elif [[ "${lower_err}" == *"no such file or directory"* ]]; then
  status="partial"
  errno_value="ENOENT"
  message="Trust list file not found"
elif [[ "${lower_err}" == *"permission denied"* || "${lower_err}" == *"operation not permitted"* ]]; then
  status="denied"
  errno_value="EACCES"
  message="Cannot access trust list file"
else
  status="error"
  errno_value=""
  message="Failed to read trust list file"
fi

bytes_read_json="null"
if [[ ${exit_code} -eq 0 ]]; then
  bytes_candidate=$(wc -c <"${target_path}" 2>/dev/null || true)
  bytes_candidate=$(printf '%s' "${bytes_candidate}" | tr -d '[:space:]')
  if [[ "${bytes_candidate}" =~ ^[0-9]+$ ]]; then
    bytes_read_json="${bytes_candidate}"
  fi
fi

hash_value=""
hash_tool=""
if [[ ${exit_code} -eq 0 ]]; then
  if command -v shasum >/dev/null 2>&1; then
    hash_value=$(shasum -a 256 "${target_path}" 2>/dev/null | awk '{print $1}')
    if [[ -n "${hash_value}" ]]; then
      hash_tool="shasum -a 256"
    fi
  elif command -v openssl >/dev/null 2>&1; then
    hash_value=$(openssl dgst -sha256 "${target_path}" 2>/dev/null | awk '{print $NF}')
    if [[ -n "${hash_value}" ]]; then
      hash_tool="openssl sha256"
    fi
  fi
fi

stdout_snippet=""
if [[ ${exit_code} -eq 0 ]]; then
  stdout_snippet="(suppressed: trust list contents not logged)"
fi

if [[ -n "${scratch_dir}" ]]; then
  if payload_tmp=$(TMPDIR="${scratch_dir}" mktemp 2>/dev/null); then
    :
  fi
fi
if [[ -z "${payload_tmp}" ]]; then
  if payload_tmp=$(mktemp 2>/dev/null); then
    :
  fi
fi

emit_args=(
  --run-mode "${run_mode}"
  --probe-name "${probe_name}"
  --probe-version "1"
  --primary-capability-id "${primary_capability_id}"
  --command "${command_executed}"
  --category "agent_policy"
  --verb "read"
  --target "${target_path}"
  --status "${status}"
  --errno "${errno_value}"
  --message "${message}"
  --raw-exit-code "${raw_exit_code}"
  --payload-stdout "${stdout_snippet}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw-field "path" "${target_path}" \
  --payload-raw-field-json "exit_code" "${exit_code}" \
  --payload-raw-field-json "bytes_read" "${bytes_read_json}" \
  --payload-raw-field "sha256" "${hash_value}" \
  --payload-raw-field "hash_tool" "${hash_tool}" \
  --payload-raw-field-json "content_logged" "false" \
  --payload-raw-field "stderr" "${stderr_text}" \
  --operation-arg "path" "${target_path}"
)

"${emit_record_bin}" "${emit_args[@]}"
