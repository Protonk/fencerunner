#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)

source "${script_dir}/library/utils.sh"

usage() {
  cat <<'USAGE' >&2
Usage: tests/run.sh [--probe <probe>]

Without arguments, runs two tiers of checks:
  1. Fast tier: light lint + static probe contract
  2. Full tier: capability map sync, schema validation, and harness smoke tests

Use --probe to run the fast tier against a single probe path or id.
USAGE
}

probe_arg=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --probe)
      if [[ $# -lt 2 ]]; then
        usage
        exit 1
      fi
      if [[ -n "${probe_arg}" ]]; then
        echo "tests/run.sh: only one --probe argument is supported" >&2
        exit 1
      fi
      probe_arg="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "tests/run.sh: unknown argument '$1'" >&2
      usage
      exit 1
      ;;
  esac
done

cd "${REPO_ROOT}"

light_lint_bin="${script_dir}/probe_contract/light_lint.sh"
if [[ ! -x "${light_lint_bin}" ]]; then
  echo "tests/run.sh: missing light lint helper at ${light_lint_bin}" >&2
  exit 1
fi

run_suite() {
  local label="$1"
  local relative_path="$2"
  local suite_script="${script_dir}/${relative_path}"
  if [[ ! -x "${suite_script}" ]]; then
    echo "Missing test script: ${suite_script}" >&2
    return 1
  fi
  echo "Running ${label}..."
  if "${suite_script}"; then
    echo "${label}: PASS"
  else
    echo "${label}: FAIL"
    return 1
  fi
  echo
  return 0
}

if [[ -n "${probe_arg}" ]]; then
  probe_script=$(resolve_probe_script_path "${REPO_ROOT}" "${probe_arg}" || true)
  if [[ -z "${probe_script}" ]]; then
    echo "tests/run.sh: unable to resolve probe '${probe_arg}'" >&2
    exit 1
  fi
  if [[ "${probe_script}" != "${REPO_ROOT}/probes/"* ]]; then
    echo "tests/run.sh: '${probe_script}' is outside probes/" >&2
    exit 1
  fi
  probe_rel=${probe_script#"${REPO_ROOT}/"}
  echo "Probe mode: ${probe_rel}"
  echo "Running light lint..."
  "${light_lint_bin}" "${probe_script}"
  echo "Running static probe contract..."
  "${script_dir}/probe_contract/static_probe_contract.sh" --probe "${probe_script}"
  echo "Probe ${probe_rel}: PASS"
  exit 0
fi

probe_scripts=()
probes_root="${REPO_ROOT}/probes"
if [[ -d "${probes_root}" ]]; then
  while IFS= read -r script; do
    probe_scripts+=("${script}")
  done < <(find "${probes_root}" -type f -name '*.sh' -print | LC_ALL=C sort)
fi

if [[ ${#probe_scripts[@]} -gt 0 ]]; then
  echo "Running light lint on ${#probe_scripts[@]} probe(s)..."
  "${light_lint_bin}" "${probe_scripts[@]}"
fi

"${script_dir}/probe_contract/static_probe_contract.sh"

second_tier_suites=(
  "capability_map_sync"
  "boundary_object_schema"
  "harness_smoke"
  "baseline_no_codex_smoke"
)

status=0
for suite in "${second_tier_suites[@]}"; do
  if ! run_suite "${suite}" "second_tier/${suite}.sh"; then
    status=1
  fi
done

exit ${status}
