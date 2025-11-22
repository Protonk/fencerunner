#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Probe contract validator (static + dynamic helpers).
#
# - Provides gate_static_probe/gate_probe functions for agents to source.
# - CLI: run static scan over all probes (default), or gate a specific probe
#   across modes with --probe.
# - Emits an embedded emit-record stub (via --emit-record-stub) for dynamic
#   validation; no separate stub file is needed.
# -----------------------------------------------------------------------------
set -euo pipefail

gate_name="validate_contract_gate"
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
repo_root=$(cd "${script_dir}/.." >/dev/null 2>&1 && pwd)

maybe_exec_fence_test() {
  local candidate
  for candidate in \
    "${repo_root}/target/debug/fence-test" \
    "${repo_root}/target/release/fence-test" \
    "${repo_root}/bin/fence-test"; do
    if [[ -x "${candidate}" ]]; then
      FENCE_TEST_FORCE_SCRIPT=1 exec "${candidate}" "$@"
    fi
  done
}

paths_helper="${repo_root}/tools/resolve_paths.sh"
modes_helper="${repo_root}/tools/list_run_modes.sh"
fence_run_bin="${repo_root}/bin/fence-run"

if [[ ! -f "${paths_helper}" ]]; then
  echo "${gate_name}: missing path helper at ${paths_helper}" >&2
  exit 1
fi
# shellcheck source=/dev/null
source "${paths_helper}"

if [[ ! -f "${modes_helper}" ]]; then
  echo "${gate_name}: missing run-modes helper at ${modes_helper}" >&2
  exit 1
fi
# shellcheck source=/dev/null
source "${modes_helper}"

usage() {
  cat <<'USAGE' >&2
Usage:
  tools/validate_contract_gate.sh                      # static scan of all probes
  tools/validate_contract_gate.sh --probe <id|path>    # static + dynamic gate for one probe
Options:
  --probe <id|path>   Target a single probe (filename or probe id).
  --static-only       Run only the static contract (skip dynamic gate).
  --modes "<list>"    Override run modes for dynamic gate (space or comma separated).
  --help              Show this message.

Internal:
  --emit-record-stub  Run the embedded emit-record stub (used by gate tooling).
USAGE
}

# ---------------- emit-record stub ----------------

emit_record_stub_main() {
  local stub_path
  stub_path=$(mktemp)
  write_emit_stub_script "${stub_path}"
  chmod +x "${stub_path}"
  "${stub_path}" "$@"
  local status=$?
  rm -f "${stub_path}"
  exit "${status}"
}

# ---------------- shared helpers ----------------

extract_probe_var() {
  local file="$1"
  local var="$2"
  local line value trimmed first last value_length
  line=$(grep -E "^[[:space:]]*${var}=" "$file" | head -n1 || true)
  if [[ -z "${line}" ]]; then
    return 1
  fi
  value=${line#*=}
  value=${value%%#*}
  value=$(printf '%s' "${value}" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')
  if [[ -n "${value}" ]]; then
    first=${value:0:1}
    last=${value: -1}
    value_length=${#value}
    if [[ "${first}" == '"' && "${last}" == '"' && ${value_length} -ge 2 ]]; then
      value=${value:1:value_length-2}
    elif [[ "${first}" == "'" && "${last}" == "'" && ${value_length} -ge 2 ]]; then
      value=${value:1:value_length-2}
    fi
  fi
  printf '%s\n' "${value}"
}

resolve_probe() {
  local identifier="$1"
  local resolved=""
  if declare -F resolve_probe_script_path >/dev/null 2>&1; then
    resolved=$(resolve_probe_script_path "${repo_root}" "${identifier}" || true)
  else
    if [[ -f "${identifier}" ]]; then
      resolved=$(portable_realpath "${identifier}")
    elif [[ -f "${repo_root}/probes/${identifier}" ]]; then
      resolved=$(portable_realpath "${repo_root}/probes/${identifier}")
    elif [[ -f "${repo_root}/probes/${identifier}.sh" ]]; then
      resolved=$(portable_realpath "${repo_root}/probes/${identifier}.sh")
    fi
  fi
  printf '%s' "${resolved}"
}

collect_probes() {
  local target_probe="$1"
  if [[ -n "${target_probe}" ]]; then
    local resolved
    resolved=$(resolve_probe "${target_probe}")
    if [[ -z "${resolved}" || ! -f "${resolved}" ]]; then
      echo "${gate_name}: unable to resolve probe '${target_probe}'" >&2
      exit 1
    fi
    if [[ "${resolved}" != "${repo_root}/probes/"* ]]; then
      echo "${gate_name}: '${resolved}' is outside probes/" >&2
      exit 1
    fi
    printf '%s\n' "${resolved}"
    return 0
  fi

  if [[ ! -d "${repo_root}/probes" ]]; then
    echo "${gate_name}: probes/ directory not found" >&2
    exit 1
  fi

  find "${repo_root}/probes" -type f -name '*.sh' -print | LC_ALL=C sort | while read -r script; do
    portable_realpath "${script}"
  done
}

gate_static_probe() {
  local probe_script="$1"
  local rel_path=${probe_script#"${repo_root}/"}
  local probe_file
  probe_file=$(basename "${probe_script}")
  local probe_id=${probe_file%.sh}
  local errors=()

  if [[ ! -x "${probe_script}" ]]; then
    errors+=("not executable (chmod +x)")
  fi

  local first_line
  first_line=$(head -n 1 "${probe_script}" || true)
  if [[ "${first_line}" != '#!/usr/bin/env bash' ]]; then
    errors+=("missing '#!/usr/bin/env bash' shebang")
  fi

  if ! grep -Eq '^[[:space:]]*set -euo pipefail' "${probe_script}"; then
    errors+=("missing 'set -euo pipefail'")
  fi

  local syntax_error
  if ! syntax_error=$(bash -n "${probe_script}" 2>&1); then
    syntax_error=${syntax_error:-"bash -n failed"}
    errors+=("syntax error: ${syntax_error}")
  fi

  local probe_name
  probe_name=$(extract_probe_var "${probe_script}" "probe_name" 2>/dev/null || true)
  if [[ -z "${probe_name}" ]]; then
    errors+=("missing probe_name assignment")
  elif [[ "${probe_name}" != "${probe_id}" ]]; then
    errors+=("probe_name '${probe_name}' does not match filename '${probe_id}'")
  fi

  local primary_capability
  primary_capability=$(extract_probe_var "${probe_script}" "primary_capability_id" 2>/dev/null || true)
  if [[ -z "${primary_capability}" ]]; then
    errors+=("missing primary_capability_id assignment")
  fi

  if [[ ${#errors[@]} -eq 0 ]]; then
    echo "${gate_name}: [PASS] ${rel_path}"
    return 0
  fi

  for err in "${errors[@]}"; do
    echo "${gate_name}: ${rel_path}: ${err}" >&2
  done
  return 1
}

write_emit_stub_script() {
  local dest="$1"
  cat >"${dest}" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail

state_dir=${PROBE_CONTRACT_GATE_STATE_DIR:-}
if [[ -z "${state_dir}" ]]; then
  echo "emit-record stub: PROBE_CONTRACT_GATE_STATE_DIR is not set" >&2
  exit 1
fi

mkdir -p "${state_dir}" >/dev/null 2>&1

counter_file="${state_dir}/emit_record_invocations"
error_file="${state_dir}/emit_record_errors.log"
status_file="${state_dir}/emit_record_status"
args_file="${state_dir}/emit_record_last_args"

fail() {
  local message="$1"
  printf '%s\n' "${message}" >>"${error_file}"
  printf '%s\n' "emit-record stub: ${message}" >&2
  exit 1
}

record_invocation() {
  local current="0"
  if [[ -f "${counter_file}" ]]; then
    current=$(cat "${counter_file}" 2>/dev/null || printf '0')
  fi
  if ! [[ "${current}" =~ ^[0-9]+$ ]]; then
    current="0"
  fi
  current=$((current + 1))
  printf '%s' "${current}" >"${counter_file}"
}

record_invocation

printf '%s\0' "$@" >"${args_file}" 2>/dev/null || true

required_run_mode=${PROBE_CONTRACT_EXPECTED_RUN_MODE:-}
required_probe_name=${PROBE_CONTRACT_EXPECTED_PROBE_NAME:-}
required_primary_capability_id=${PROBE_CONTRACT_EXPECTED_PRIMARY_CAPABILITY_ID:-}
capabilities_json=${PROBE_CONTRACT_CAPABILITIES_JSON:-}
capabilities_adapter=${PROBE_CONTRACT_CAPABILITIES_ADAPTER:-}

ensure_jq() {
  if ! command -v jq >/dev/null 2>&1; then
    fail "jq is not available for JSON validation"
  fi
}

ensure_jq

run_mode=""
probe_name=""
probe_version=""
primary_capability_id=""
command_value=""
category=""
verb=""
target=""
status_value=""
errno_value=""
message_value=""
payload_file=""
operation_args=""
operation_args_file=""
raw_exit_code=""
payload_seen="false"
payload_inline_seen="false"
operation_args_seen="false"

run_mode_set="false"
probe_name_set="false"
probe_version_set="false"
primary_capability_id_set="false"
command_set="false"
category_set="false"
verb_set="false"
target_set="false"
status_set="false"
payload_file_set="false"
operation_args_set="false"
operation_args_file_set="false"
raw_exit_code_set="false"

assign_flag() {
  local var_name="$1"
  local set_flag_name="$2"
  local human_name="$3"
  local value="$4"
  local allow_empty="$5"

  if [[ "${!set_flag_name-}" == "true" ]]; then
    fail "${human_name} provided multiple times"
  fi
  if [[ "${allow_empty}" != "allow-empty" && -z "${value}" ]]; then
    fail "${human_name} cannot be empty"
  fi
  printf -v "${var_name}" '%s' "${value}"
  printf -v "${set_flag_name}" '%s' "true"
}

secondary_capability_ids=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --run-mode)
      [[ $# -ge 2 ]] || fail "--run-mode requires a value"
      assign_flag run_mode run_mode_set "--run-mode" "$2" "no-empty"
      shift 2
      ;;
    --probe-name)
      [[ $# -ge 2 ]] || fail "--probe-name requires a value"
      assign_flag probe_name probe_name_set "--probe-name" "$2" "no-empty"
      shift 2
      ;;
    --probe-version)
      [[ $# -ge 2 ]] || fail "--probe-version requires a value"
      assign_flag probe_version probe_version_set "--probe-version" "$2" "no-empty"
      shift 2
      ;;
    --primary-capability-id)
      [[ $# -ge 2 ]] || fail "--primary-capability-id requires a value"
      assign_flag primary_capability_id primary_capability_id_set "--primary-capability-id" "$2" "no-empty"
      shift 2
      ;;
    --secondary-capability-id)
      [[ $# -ge 2 ]] || fail "--secondary-capability-id requires a value"
      secondary_capability_ids+=("$2")
      shift 2
      ;;
    --command)
      [[ $# -ge 2 ]] || fail "--command requires a value"
      assign_flag command_value command_set "--command" "$2" "no-empty"
      shift 2
      ;;
    --category)
      [[ $# -ge 2 ]] || fail "--category requires a value"
      assign_flag category category_set "--category" "$2" "no-empty"
      shift 2
      ;;
    --verb)
      [[ $# -ge 2 ]] || fail "--verb requires a value"
      assign_flag verb verb_set "--verb" "$2" "no-empty"
      shift 2
      ;;
    --target)
      [[ $# -ge 2 ]] || fail "--target requires a value"
      assign_flag target target_set "--target" "$2" "no-empty"
      shift 2
      ;;
    --status)
      [[ $# -ge 2 ]] || fail "--status requires a value"
      assign_flag status_value status_set "--status" "$2" "no-empty"
      shift 2
      ;;
    --errno)
      [[ $# -ge 2 ]] || fail "--errno requires a value"
      errno_value="$2"
      shift 2
      ;;
    --message)
      [[ $# -ge 2 ]] || fail "--message requires a value"
      message_value="$2"
      shift 2
      ;;
    --payload-file)
      [[ $# -ge 2 ]] || fail "--payload-file requires a path"
      assign_flag payload_file payload_file_set "--payload-file" "$2" "no-empty"
      payload_seen="true"
      shift 2
      ;;
    --payload-stdout|--payload-stdout-file|--payload-stderr|--payload-stderr-file)
      [[ $# -ge 2 ]] || fail "$1 requires a value"
      payload_seen="true"
      payload_inline_seen="true"
      shift 2
      ;;
    --payload-raw)
      [[ $# -ge 2 ]] || fail "--payload-raw requires JSON"
      payload_seen="true"
      payload_inline_seen="true"
      if ! printf '%s' "$2" | jq -e 'type=="object"' >/dev/null 2>&1; then
        fail "--payload-raw must be a JSON object"
      fi
      shift 2
      ;;
    --payload-raw-file)
      [[ $# -ge 2 ]] || fail "--payload-raw-file requires a path"
      payload_seen="true"
      payload_inline_seen="true"
      raw_file="$2"
      if [[ ! -f "${raw_file}" ]]; then
        fail "payload raw file '${raw_file}' does not exist"
      fi
      raw_size=$(wc -c <"${raw_file}" 2>/dev/null || printf '0')
      if ! [[ "${raw_size}" =~ ^[0-9]+$ ]]; then
        raw_size=0
      fi
      if (( raw_size > 1048576 )); then
        fail "payload raw file is larger than 1048576 bytes"
      fi
      if ! jq -e 'type=="object"' "${raw_file}" >/dev/null 2>&1; then
        fail "--payload-raw-file must contain a JSON object"
      fi
      shift 2
      ;;
    --payload-raw-field)
      [[ $# -ge 3 ]] || fail "--payload-raw-field requires KEY VALUE"
      payload_seen="true"
      payload_inline_seen="true"
      shift 3
      ;;
    --payload-raw-field-json)
      [[ $# -ge 3 ]] || fail "--payload-raw-field-json requires KEY JSON_VALUE"
      payload_seen="true"
      payload_inline_seen="true"
      if ! printf '%s' "$2" | jq -e . >/dev/null 2>&1; then
        fail "--payload-raw-field-json value is not valid JSON"
      fi
      shift 3
      ;;
    --payload-raw-null)
      [[ $# -ge 2 ]] || fail "--payload-raw-null requires KEY"
      payload_seen="true"
      payload_inline_seen="true"
      shift 2
      ;;
    --payload-raw-list)
      [[ $# -ge 3 ]] || fail "--payload-raw-list requires KEY VALUES"
      payload_seen="true"
      payload_inline_seen="true"
      shift 3
      ;;
    --operation-args)
      [[ $# -ge 2 ]] || fail "--operation-args requires JSON"
      operation_args_seen="true"
      assign_flag operation_args operation_args_set "--operation-args" "$2" "no-empty"
      if ! printf '%s' "$2" | jq -e 'type=="object"' >/dev/null 2>&1; then
        fail "--operation-args must be a JSON object"
      fi
      shift 2
      ;;
    --operation-args-file)
      [[ $# -ge 2 ]] || fail "--operation-args-file requires a path"
      operation_args_seen="true"
      assign_flag operation_args_file operation_args_file_set "--operation-args-file" "$2" "no-empty"
      if [[ ! -f "$2" ]]; then
        fail "operation args file '$2' does not exist"
      fi
      if ! jq -e 'type=="object"' "$2" >/dev/null 2>&1; then
        fail "--operation-args-file must be a JSON object"
      fi
      shift 2
      ;;
    --operation-arg)
      [[ $# -ge 3 ]] || fail "--operation-arg requires KEY VALUE"
      operation_args_seen="true"
      shift 3
      ;;
    --operation-arg-json)
      [[ $# -ge 3 ]] || fail "--operation-arg-json requires KEY JSON_VALUE"
      operation_args_seen="true"
      if ! printf '%s' "$3" | jq -e . >/dev/null 2>&1; then
        fail "--operation-arg-json value is not valid JSON"
      fi
      shift 3
      ;;
    --operation-arg-null)
      [[ $# -ge 2 ]] || fail "--operation-arg-null requires KEY"
      operation_args_seen="true"
      shift 2
      ;;
    --operation-arg-list)
      [[ $# -ge 3 ]] || fail "--operation-arg-list requires KEY VALUES"
      operation_args_seen="true"
      shift 3
      ;;
    --raw-exit-code)
      [[ $# -ge 2 ]] || fail "--raw-exit-code requires a value"
      assign_flag raw_exit_code raw_exit_code_set "--raw-exit-code" "$2" "no-empty"
      shift 2
      ;;
    --*)
      fail "unknown flag '$1'"
      ;;
    *)
      fail "unexpected positional argument '$1'"
      ;;
  esac
done

require_flag_value() {
  local value="$1"
  local human_name="$2"
  if [[ -z "${value}" ]]; then
    fail "${human_name} is required"
  fi
}

require_flag_value "${run_mode}" "--run-mode"
require_flag_value "${probe_name}" "--probe-name"
require_flag_value "${probe_version}" "--probe-version"
require_flag_value "${primary_capability_id}" "--primary-capability-id"
require_flag_value "${command_value}" "--command"
require_flag_value "${category}" "--category"
require_flag_value "${verb}" "--verb"
require_flag_value "${target}" "--target"
require_flag_value "${status_value}" "--status"
require_flag_value "${raw_exit_code}" "--raw-exit-code"

if [[ "${payload_seen}" != "true" && -z "${payload_file}" ]]; then
  fail "payload flags are required"
fi

if [[ -n "${required_run_mode}" && "${run_mode}" != "${required_run_mode}" ]]; then
  fail "--run-mode '${run_mode}' does not match expected '${required_run_mode}'"
fi

if [[ -n "${required_probe_name}" && "${probe_name}" != "${required_probe_name}" ]]; then
  fail "--probe-name '${probe_name}' does not match expected '${required_probe_name}'"
fi

if [[ -n "${required_primary_capability_id}" && "${primary_capability_id}" != "${required_primary_capability_id}" ]]; then
  fail "--primary-capability-id '${primary_capability_id}' does not match expected '${required_primary_capability_id}'"
fi

case "${status_value}" in
  success|denied|partial|error)
    ;;
  *)
    fail "--status '${status_value}' is not in the allowed set"
    ;;
esac

if ! [[ "${raw_exit_code}" =~ ^-?[0-9]+$ ]]; then
  fail "--raw-exit-code '${raw_exit_code}' is not an integer"
fi

if [[ -n "${payload_file}" && "${payload_inline_seen}" == "true" ]]; then
  fail "--payload-file cannot be combined with inline payload flags"
fi

if [[ -n "${payload_file}" ]]; then
  if [[ ! -f "${payload_file}" ]]; then
    fail "payload file '${payload_file}' does not exist"
  fi

  payload_size=$(wc -c <"${payload_file}" 2>/dev/null || printf '0')
  if ! [[ "${payload_size}" =~ ^[0-9]+$ ]]; then
    payload_size=0
  fi
  max_payload_bytes=$((1024 * 1024))
  if (( payload_size > max_payload_bytes )); then
    fail "payload file is larger than ${max_payload_bytes}"
  fi

  if ! jq -e 'type == "object" and has("stdout_snippet") and has("stderr_snippet") and has("raw") and ((.stdout_snippet | type == "string") or (.stdout_snippet == null)) and ((.stderr_snippet | type == "string") or (.stderr_snippet == null)) and (.raw | type == "object")' "${payload_file}" >/dev/null; then
    fail "payload JSON missing required fields"
  fi
fi

if [[ "${operation_args_seen}" != "true" ]]; then
  fail "operation args flags are required"
fi

if [[ -n "${capabilities_json}" && -n "${capabilities_adapter}" && -x "${capabilities_adapter}" && -f "${capabilities_json}" ]]; then
  if ! capability_map=$("${capabilities_adapter}" "${capabilities_json}" 2>/dev/null); then
    fail "capability catalog validation failed"
  fi
  if ! printf '%s' "${capability_map}" | jq -e --arg id "${primary_capability_id}" 'has($id)' >/dev/null 2>&1; then
    fail "unknown primary_capability_id '${primary_capability_id}'"
  fi
fi

printf 'ok\n' >"${status_file}"

printf '{"probe_contract_gate":"validated"}\n'

STUB
  chmod +x "${dest}"
}

if [[ "${1-}" == "--emit-record-stub" ]]; then
  shift
  emit_record_stub_main "$@"
fi

if [[ "${BASH_SOURCE[0]}" == "${0}" && -z "${FENCE_TEST_FORCE_SCRIPT:-}" ]]; then
  maybe_exec_fence_test "$@"
fi

run_with_timeout() {
  local seconds="$1"
  shift
  if command -v gtimeout >/dev/null 2>&1; then
    gtimeout "${seconds}" "$@"
    return $?
  fi
  if command -v timeout >/dev/null 2>&1; then
    timeout "${seconds}" "$@"
    return $?
  fi
  local pid
  "$@" &
  pid=$!
  local elapsed=0
  while kill -0 "${pid}" >/dev/null 2>&1; do
    if (( elapsed >= seconds )); then
      kill "${pid}" >/dev/null 2>&1 || true
      wait "${pid}" 2>/dev/null || true
      return 124
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done
  wait "${pid}"
}

run_dynamic_gate() {
  local probe_path="$1"
  local probe_id="$2"
  local expected_probe_name="$3"
  local expected_primary_capability_id="$4"
  local run_mode="$5"

  if [[ ! -x "${fence_run_bin}" ]]; then
    echo "${gate_name}: missing fence-run helper at ${fence_run_bin}" >&2
    return 1
  fi

  local shadow_root
  shadow_root=$(mktemp -d "${TMPDIR:-/tmp}/probe-contract-shadow.XXXXXX")

  mkdir -p "${shadow_root}/bin" "${shadow_root}/probes" "${shadow_root}/tmp"
  # Provide repo root sentinels expected by find_repo_root.
  if [[ -f "${repo_root}/bin/.gitkeep" ]]; then
    cp "${repo_root}/bin/.gitkeep" "${shadow_root}/bin/.gitkeep"
  fi
  if [[ -f "${repo_root}/Makefile" ]]; then
    ln -s "${repo_root}/Makefile" "${shadow_root}/Makefile" 2>/dev/null || cp "${repo_root}/Makefile" "${shadow_root}/Makefile"
  fi

  local probe_filename
  probe_filename=$(basename "${probe_path}")
  local shadow_probe="${shadow_root}/probes/${probe_filename}"
  cp "${probe_path}" "${shadow_probe}"
  chmod +x "${shadow_probe}"

  local helper
  for helper in "${repo_root}"/bin/*; do
    local helper_name
    helper_name=$(basename "${helper}")
    if [[ "${helper_name}" == "emit-record" ]]; then
      continue
    fi
    if [[ -d "${helper}" ]]; then
      continue
    fi
    ln -s "${helper}" "${shadow_root}/bin/${helper_name}"
  done

  local stub_path="${shadow_root}/bin/emit-record"
  write_emit_stub_script "${stub_path}"

  local stub_state="${shadow_root}/.pcg_state"
  mkdir -p "${stub_state}"

  local probe_stdout
  probe_stdout=$(mktemp "${TMPDIR:-/tmp}/probe-contract-stdout.XXXXXX")
  local probe_stderr
  probe_stderr=$(mktemp "${TMPDIR:-/tmp}/probe-contract-stderr.XXXXXX")

  local path_prefix="${shadow_root}/bin:${PATH}"

  local modes_arg=()
  case "${run_mode}" in
    baseline) modes_arg+=("baseline");;
    codex-sandbox) modes_arg+=("codex-sandbox");;
    codex-full) modes_arg+=("codex-full");;
    *) modes_arg+=("${run_mode}");;
  esac

  local run_env=(env
    PATH="${path_prefix}"
    HOME="${shadow_root}"
    CODEX_FENCE_ROOT="${shadow_root}"
    PROBE_CONTRACT_GATE_STATE_DIR="${stub_state}"
    PROBE_CONTRACT_EXPECTED_RUN_MODE="${run_mode}"
    PROBE_CONTRACT_CAPABILITIES_JSON="${repo_root}/schema/capabilities.json"
    PROBE_CONTRACT_CAPABILITIES_ADAPTER="${repo_root}/tools/adapt_capabilities.sh")

  if [[ -n "${expected_probe_name}" ]]; then
    run_env+=("PROBE_CONTRACT_EXPECTED_PROBE_NAME=${expected_probe_name}")
  fi
  if [[ -n "${expected_primary_capability_id}" ]]; then
    run_env+=("PROBE_CONTRACT_EXPECTED_PRIMARY_CAPABILITY_ID=${expected_primary_capability_id}")
  fi

  # Run through fence-run so the gate matches real execution environment.
  run_env+=("${fence_run_bin}" "--workspace-root" "${shadow_root}" "${run_mode}" "${shadow_probe}")

  if ! run_with_timeout 5 "${run_env[@]}" >"${probe_stdout}" 2>"${probe_stderr}"; then
    local exit_code=$?
    local stub_errors
    if [[ -s "${stub_state}/emit_record_errors.log" ]]; then
      stub_errors=$(cat "${stub_state}/emit_record_errors.log")
    fi
    if [[ -n "${stub_errors}" ]]; then
      echo "${gate_name}: dynamic gate failed for ${probe_id}: ${stub_errors}" >&2
    elif [[ ${exit_code} -eq 124 ]]; then
      echo "${gate_name}: dynamic gate timed out after 5s for ${probe_id}" >&2
    else
      echo "${gate_name}: dynamic gate failed for ${probe_id} (exit ${exit_code})" >&2
    fi
    if [[ -s "${probe_stderr}" ]]; then
      echo "probe stderr:" >&2
      tail -n 20 "${probe_stderr}" >&2
    fi
    rm -rf "${shadow_root}" "${probe_stdout}" "${probe_stderr}"
    return 1
  fi

  local invocation_file="${stub_state}/emit_record_invocations"
  if [[ ! -f "${invocation_file}" ]]; then
    echo "${gate_name}: dynamic gate failed for ${probe_id}: emit-record not invoked" >&2
    rm -rf "${shadow_root}" "${probe_stdout}" "${probe_stderr}"
    return 1
  fi
  local invocation_count
  invocation_count=$(cat "${invocation_file}" 2>/dev/null || printf '0')
  if ! [[ "${invocation_count}" =~ ^[0-9]+$ ]]; then
    echo "${gate_name}: dynamic gate failed for ${probe_id}: invalid invocation counter" >&2
    rm -rf "${shadow_root}" "${probe_stdout}" "${probe_stderr}"
    return 1
  fi
  if [[ "${invocation_count}" -eq 0 ]]; then
    echo "${gate_name}: dynamic gate failed for ${probe_id}: emit-record not called" >&2
    rm -rf "${shadow_root}" "${probe_stdout}" "${probe_stderr}"
    return 1
  fi
  if [[ "${invocation_count}" -ne 1 ]]; then
    echo "${gate_name}: dynamic gate failed for ${probe_id}: emit-record called ${invocation_count} times" >&2
    rm -rf "${shadow_root}" "${probe_stdout}" "${probe_stderr}"
    return 1
  fi

  local stub_errors_file="${stub_state}/emit_record_errors.log"
  if [[ -s "${stub_errors_file}" ]]; then
    echo "${gate_name}: dynamic gate failed for ${probe_id}: $(tr '\n' ' ' <"${stub_errors_file}")" >&2
    rm -rf "${shadow_root}" "${probe_stdout}" "${probe_stderr}"
    return 1
  fi

  local stub_status_file="${stub_state}/emit_record_status"
  if [[ ! -f "${stub_status_file}" ]]; then
    echo "${gate_name}: dynamic gate failed for ${probe_id}: stub status missing" >&2
    rm -rf "${shadow_root}" "${probe_stdout}" "${probe_stderr}"
    return 1
  fi
  if ! grep -q "^ok$" "${stub_status_file}" >/dev/null 2>&1; then
    echo "${gate_name}: dynamic gate failed for ${probe_id}: stub status not ok" >&2
    rm -rf "${shadow_root}" "${probe_stdout}" "${probe_stderr}"
    return 1
  fi

  rm -rf "${shadow_root}" "${probe_stdout}" "${probe_stderr}"
  echo "${gate_name}: dynamic gate passed for ${probe_id} (${run_mode})"
  return 0
}

gate_probe() {
  local probe_identifier="$1"
  shift || true
  local modes_arg="$*"

  local probe_path
  probe_path=$(resolve_probe "${probe_identifier}")
  if [[ -z "${probe_path}" || ! -f "${probe_path}" ]]; then
    echo "${gate_name}: unable to resolve probe '${probe_identifier}'" >&2
    return 1
  fi
  local probe_file
  probe_file=$(basename "${probe_path}")
  local probe_id=${probe_file%.sh}

  if ! gate_static_probe "${probe_path}"; then
    echo "${gate_name}: static gate failed for ${probe_id}" >&2
    return 1
  fi

  local expected_probe_name
  expected_probe_name=$(extract_probe_var "${probe_path}" "probe_name" || true)
  local expected_primary_capability_id
  expected_primary_capability_id=$(extract_probe_var "${probe_path}" "primary_capability_id" || true)

  local modes=()
  local mode_override="${PROBE_CONTRACT_MODES:-}"
  if [[ -n "${modes_arg}" ]]; then
    mode_override="${modes_arg}"
  fi

  if [[ -n "${mode_override}" ]]; then
    local normalized
    normalized=$(printf '%s' "${mode_override}" | tr ',' ' ' | tr -s ' ' ' ')
    for m in ${normalized}; do
      [[ -n "${m}" ]] && modes+=("${m}")
    done
  elif declare -F contract_gate_modes >/dev/null 2>&1; then
    while IFS= read -r mode; do
      [[ -n "${mode}" ]] && modes+=("${mode}")
    done < <(contract_gate_modes)
  fi
  if [[ ${#modes[@]} -eq 0 ]]; then
    modes=("baseline" "codex-sandbox" "codex-full")
  fi

  local mode
  for mode in "${modes[@]}"; do
    if ! run_dynamic_gate "${probe_path}" "${probe_id}" "${expected_probe_name}" "${expected_primary_capability_id}" "${mode}"; then
      echo "${gate_name}: dynamic gate failed for ${probe_id} (${mode})" >&2
      return 1
    fi
  done

  echo "${gate_name}: all gates passed for ${probe_id}"
}

main() {
  if [[ $# -eq 0 ]]; then
    local failures=0
    while read -r probe_script; do
      if ! gate_static_probe "${probe_script}"; then
        failures=1
      fi
    done < <(collect_probes "")
    exit "${failures}"
  fi

  local target_probe=""
  local static_only="false"
  local modes_arg=""

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --probe)
        [[ $# -ge 2 ]] || { usage; exit 1; }
        target_probe="$2"
        shift 2
        ;;
      --static-only)
        static_only="true"
        shift
        ;;
      --modes)
        [[ $# -ge 2 ]] || { usage; exit 1; }
        modes_arg="$2"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        usage
        exit 1
        ;;
    esac
  done

  if [[ -z "${target_probe}" ]]; then
    usage
    exit 1
  fi

  if [[ "${static_only}" == "true" ]]; then
    local resolved
    resolved=$(resolve_probe "${target_probe}")
    if [[ -z "${resolved}" ]]; then
      echo "${gate_name}: unable to resolve probe '${target_probe}'" >&2
      exit 1
    fi
    gate_static_probe "${resolved}"
    exit $?
  fi

  gate_probe "${target_probe}" "${modes_arg}"
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  main "$@"
fi
