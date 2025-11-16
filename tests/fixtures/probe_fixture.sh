#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
repo_root=$(cd "${script_dir}/.." >/dev/null 2>&1 && pwd)
if [[ ! -x "${repo_root}/bin/emit-record" ]]; then
  repo_root=$(cd "${script_dir}/../.." >/dev/null 2>&1 && pwd)
fi
emit_record_bin="${repo_root}/bin/emit-record"

probe_name="tests_fixture_probe"
run_mode="${FENCE_RUN_MODE:-baseline}"
primary_capability_id="cap_fs_read_workspace_tree"
workspace_tmp=$(mktemp -d)
target_file="${workspace_tmp}/fixture.txt"
trap 'rm -rf "${workspace_tmp}"' EXIT

printf 'fixture-line' > "${target_file}"
command_executed="printf fixture-line > ${target_file}"

payload_tmp=$(mktemp)
trap 'rm -rf "${workspace_tmp}" "${payload_tmp}"' EXIT

jq -n --arg stdout_snippet "fixture ok" --arg stderr_snippet "" --argjson raw '{"probe":"fixture"}' \
  '{stdout_snippet: $stdout_snippet, stderr_snippet: $stderr_snippet, raw: $raw}' > "${payload_tmp}"

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "read" \
  --target "${target_file}" \
  --status success \
  --raw-exit-code 0 \
  --payload-file "${payload_tmp}" \
  --operation-args '{"fixture":true}'
