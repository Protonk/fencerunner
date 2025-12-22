#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Minimal probe shim used by integration tests. It touches only temporary files
# and emits a deterministic boundary object so suites can assert harness output.
# -----------------------------------------------------------------------------
set -euo pipefail

# Find the repo root by walking up until bin/emit-record is found.
# This mirrors how real probes discover helper binaries.
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
repo_root_candidate="${script_dir}"
repo_root=""
while [[ -z "${repo_root}" && "${repo_root_candidate}" != "/" ]]; do
  # Walk upward until bin/emit-record appears—this anchors the repo root.
  if [[ -x "${repo_root_candidate}/bin/emit-record" ]]; then
    repo_root="${repo_root_candidate}"
    break
  fi
  repo_root_candidate=$(cd "${repo_root_candidate}/.." >/dev/null 2>&1 && pwd)
done
if [[ -z "${repo_root}" ]]; then
  echo "minimal_probe: unable to locate repo root" >&2
  exit 1
fi

# Prefer target builds during tests; fall back to the synced bin/ helper.
emit_record_bin="${repo_root}/bin/emit-record"
target_debug="${repo_root}/target/debug/emit-record"
target_release="${repo_root}/target/release/emit-record"
if [[ -x "${target_debug}" ]]; then
  emit_record_bin="${target_debug}"
elif [[ -x "${target_release}" ]]; then
  emit_record_bin="${target_release}"
fi

probe_name="tests_fixture_probe"
primary_capability_id="cap_fs_read_workspace_tree"
workspace_tmp=$(mktemp -d)
target_file="${workspace_tmp}/fixture.txt"
# Always clean up temp files to keep the workspace tidy.
trap 'rm -rf "${workspace_tmp}"' EXIT

printf 'fixture-line' > "${target_file}"
# Mirror what bin/probe-exec would capture so the record looks realistic.
command_executed="printf fixture-line > ${target_file}"

# Emit the same boundary object a real probe would create.
"${emit_record_bin}" \
  --probe-name "${probe_name}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --operation-kind "fs.read" \
  --target "${target_file}" \
  --outcome success \
  --exit-code 0 \
  --payload-stdout "fixture ok" \
  --payload-stderr "" \
  --payload-raw-field "probe" "fixture" \
  --operation-arg-json "fixture" "true"
