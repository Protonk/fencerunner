#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Minimal probe shim used by integration tests. It touches only temporary files
# and emits a deterministic boundary object so suites can assert harness output.
# -----------------------------------------------------------------------------
set -euo pipefail

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

emit_record_bin="${repo_root}/bin/emit-record"
target_debug="${repo_root}/target/debug/emit-record"
target_release="${repo_root}/target/release/emit-record"
if [[ -x "${target_debug}" ]]; then
  emit_record_bin="${target_debug}"
elif [[ -x "${target_release}" ]]; then
  emit_record_bin="${target_release}"
fi

probe_name="tests_fixture_probe"
run_mode="${FENCE_RUN_MODE:-baseline}"
primary_capability_id="cap_fs_read_workspace_tree"
workspace_tmp=$(mktemp -d)
target_file="${workspace_tmp}/fixture.txt"
trap 'rm -rf "${workspace_tmp}"' EXIT

printf 'fixture-line' > "${target_file}"
# Mirror what bin/fence-run would capture so the record looks realistic.
command_executed="printf fixture-line > ${target_file}"

# Emit the same boundary object a real probe would create.
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
  --payload-stdout "fixture ok" \
  --payload-stderr "" \
  --payload-raw-field "probe" "fixture" \
  --operation-arg-json "fixture" "true"
