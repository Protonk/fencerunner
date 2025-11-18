#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
# shellcheck source=tests/library/utils.sh
source "${script_dir}/../library/utils.sh"

cd "${REPO_ROOT}"

echo "baseline_no_codex_smoke: checking harness without codex CLI"

fixture_name="tests_fixture_probe"
probe_path="probes/regression/filesystem/${fixture_name}.sh"
fixture_source="tests/library/fixtures/probe_fixture.sh"

if [[ -e "${probe_path}" ]]; then
  echo "baseline_no_codex_smoke: fixture probe already exists at ${probe_path}" >&2
  exit 1
fi

cp "${fixture_source}" "${probe_path}"
chmod +x "${probe_path}"
cleanup() {
  rm -f "${probe_path}"
  rm -rf "${temp_path}" "${output_tmp}"
}
trap cleanup EXIT

temp_path=$(mktemp -d)
output_tmp=$(mktemp)
jq_path=$(command -v jq 2>/dev/null || true)
if [[ -n "${jq_path}" ]]; then
  ln -s "${jq_path}" "${temp_path}/jq" 2>/dev/null || cp "${jq_path}" "${temp_path}/jq"
fi

# Remove the directory that currently contains codex from PATH (if any) and
# prepend an empty directory to ensure codex cannot be resolved.
codex_path=$(command -v codex 2>/dev/null || true)
original_path="${PATH}"
sanitized_path="${original_path}"
if [[ -n "${codex_path}" ]]; then
  codex_dir=$(cd "$(dirname "${codex_path}")" >/dev/null 2>&1 && pwd)
  sanitized_parts=()
  IFS=':' read -r -a path_entries <<< "${original_path}"
  for entry in "${path_entries[@]}"; do
    if [[ -n "${codex_dir}" && "${entry}" == "${codex_dir}" ]]; then
      continue
    fi
    sanitized_parts+=("${entry}")
  done
  sanitized_path=""
  for entry in "${sanitized_parts[@]}"; do
    if [[ -z "${entry}" ]]; then
      continue
    fi
    if [[ -z "${sanitized_path}" ]]; then
      sanitized_path="${entry}"
    else
      sanitized_path="${sanitized_path}:${entry}"
    fi
  done
  if [[ -z "${sanitized_path}" ]]; then
    sanitized_path="${original_path}"
  fi
fi

PATH="${temp_path}:${sanitized_path}"
export PATH
hash -r

bin/fence-run baseline "${fixture_name}" > "${output_tmp}"

if bin/fence-run codex-sandbox "${fixture_name}" >/dev/null 2>&1; then
  echo "baseline_no_codex_smoke: expected codex-sandbox to fail without codex but it succeeded" >&2
  exit 1
fi

jq -e '
  .probe.id == "tests_fixture_probe" and
  .operation.category == "fs" and
  .result.observed_result == "success"
' "${output_tmp}" >/dev/null

echo "baseline_no_codex_smoke: PASS"
