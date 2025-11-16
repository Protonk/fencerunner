#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
# shellcheck source=tests/lib/utils.sh
source "${script_dir}/lib/utils.sh"

cd "${REPO_ROOT}"

payload_tmp=$(mktemp)
record_tmp=$(mktemp)
trap 'rm -f "${payload_tmp}" "${record_tmp}"' EXIT

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
  (.stack | type == "object") and
  (.probe.id == "schema_test_fixture" and
   .probe.version == "1" and
   .probe.primary_capability_id == "cap_fs_read_workspace_tree" and
   (.probe.secondary_capability_ids | type == "array")) and
  (.run.mode as $mode | ($mode == "baseline" or $mode == "codex-sandbox" or $mode == "codex-full")) and
  (.run | has("workspace_root") and has("command") and has("observed_at")) and
  (.operation.category == "fs" and .operation.verb == "read" and .operation.target == "/dev/null" and (.operation.args | type == "object")) and
  (.result.observed_result == "success" or .result.observed_result == "denied" or .result.observed_result == "partial" or .result.observed_result == "error") and
  (.result | has("raw_exit_code") and has("errno") and has("message") and has("duration_ms") and has("error_detail")) and
  (.payload.stdout_snippet == "fixture-stdout" and .payload.stderr_snippet == "fixture-stderr" and (.payload.raw | type == "object"))
' "${record_tmp}" >/dev/null

echo "boundary_object_schema: PASS"
