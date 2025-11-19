#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Executes bin/emit-record once and verifies the resulting boundary object still
# matches the cfbo-v1 schema. Acts as a safety net for accidental schema drift.
# -----------------------------------------------------------------------------
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)

source "${script_dir}/../library/utils.sh"

cd "${REPO_ROOT}"

payload_tmp=$(mktemp)
record_tmp=$(mktemp)
trap 'rm -f "${payload_tmp}" "${record_tmp}"' EXIT

# Produce a tiny but representative payload fixture without touching the probes.
cat <<'JSON' > "${payload_tmp}"
{
  "stdout_snippet": "fixture-stdout",
  "stderr_snippet": "fixture-stderr",
  "raw": {
    "detail": "schema-test"
  }
}
JSON

bin/emit-record \
  --run-mode baseline \
  --probe-name schema_test_fixture \
  --probe-version 1 \
  --primary-capability-id cap_fs_read_workspace_tree \
  --command "printf fixture" \
  --category fs \
  --verb read \
  --target /dev/null \
  --status success \
  --raw-exit-code 0 \
  --message "fixture" \
  --payload-file "${payload_tmp}" \
  --operation-args '{"fixture":true}' > "${record_tmp}"

jq -e '
  .schema_version == "cfbo-v1" and
  (has("capabilities_schema_version") and
    ((.capabilities_schema_version | type) as $csv_type |
      ($csv_type == "string" or $csv_type == "number" or $csv_type == "null"))
  ) and
  (.stack | type == "object") and
  (.probe.id == "schema_test_fixture" and
   .probe.version == "1" and
   .probe.primary_capability_id == "cap_fs_read_workspace_tree" and
   (.probe.secondary_capability_ids | type == "array")) and
  (.run.mode as $mode | ($mode == "baseline" or $mode == "codex-sandbox" or $mode == "codex-full")) and
  (.run | has("workspace_root") and has("command")) and
  (.operation.category == "fs" and .operation.verb == "read" and .operation.target == "/dev/null" and (.operation.args | type == "object")) and
  (.result.observed_result == "success" or .result.observed_result == "denied" or .result.observed_result == "partial" or .result.observed_result == "error") and
  (.result | has("raw_exit_code") and has("errno") and has("message") and has("duration_ms") and has("error_detail")) and
  (.payload.stdout_snippet == "fixture-stdout" and .payload.stderr_snippet == "fixture-stderr" and (.payload.raw | type == "object")) and
  (.capability_context | type == "object") and
  (.capability_context | has("primary") and has("secondary")) and
  (.capability_context.primary.id == "cap_fs_read_workspace_tree") and
  (.capability_context.primary | has("category") and has("layer")) and
  (.capability_context.secondary | type == "array")
' "${record_tmp}" >/dev/null
# jq -e exits non-zero if any invariant fails, propagating failure to the suite.

"${REPO_ROOT}/tests/library/json_schema_validator.sh" \
  "${REPO_ROOT}/schema/boundary_object.json" \
  "${record_tmp}"

echo "boundary_object_schema: PASS"
