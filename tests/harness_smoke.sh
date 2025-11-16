#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
# shellcheck source=tests/lib/utils.sh
source "${script_dir}/lib/utils.sh"

cd "${REPO_ROOT}"

fixture_name="tests_fixture_probe"
probe_path="probes/${fixture_name}.sh"
fixture_source="tests/fixtures/probe_fixture.sh"

if [[ -e "${probe_path}" ]]; then
  echo "harness_smoke: fixture probe already exists at ${probe_path}" >&2
  exit 1
fi

cp "${fixture_source}" "${probe_path}"
chmod +x "${probe_path}"
trap 'rm -f "${probe_path}"' EXIT

output_tmp=$(mktemp)
trap 'rm -f "${probe_path}" "${output_tmp}"' EXIT

bin/fence-run baseline "${fixture_name}" > "${output_tmp}"

jq -e '
  .probe.id == "tests_fixture_probe" and
  .operation.category == "fs" and
  .result.observed_result == "success" and
  (.payload.raw.probe == "fixture")
' "${output_tmp}" >/dev/null

echo "harness_smoke: PASS"
