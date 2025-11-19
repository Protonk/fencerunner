#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)

source "${script_dir}/../library/utils.sh"

usage() {
  cat <<'USAGE' >&2
Usage: tests/probe_contract/static_probe_contract.sh [--probe <probe>]

Without arguments, all probes under probes/ are validated. Use --probe to
restrict the run to a single probe id or path.
USAGE
}

target_probe=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --probe)
      if [[ $# -lt 2 ]]; then
        usage
        exit 1
      fi
      if [[ -n "${target_probe}" ]]; then
        echo "static_probe_contract: only one --probe value is supported" >&2
        exit 1
      fi
      target_probe="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "static_probe_contract: unknown argument '$1'" >&2
      usage
      exit 1
      ;;
  esac
done

cd "${REPO_ROOT}"

probe_scripts=()
if [[ -n "${target_probe}" ]]; then
  resolved_probe=$(resolve_probe_script_path "${REPO_ROOT}" "${target_probe}" || true)
  if [[ -z "${resolved_probe}" ]]; then
    echo "static_probe_contract: unable to resolve probe '${target_probe}'" >&2
    exit 1
  fi
  if [[ "${resolved_probe}" != "${REPO_ROOT}/probes/"* ]]; then
    echo "static_probe_contract: '${resolved_probe}' is outside probes/" >&2
    exit 1
  fi
  probe_scripts=("${resolved_probe}")
else
  probes_root="${REPO_ROOT}/probes"
  if [[ -d "${probes_root}" ]]; then
    while IFS= read -r script; do
      probe_scripts+=("${script}")
    done < <(find "${probes_root}" -type f -name '*.sh' -print | LC_ALL=C sort)
  fi
fi

if [[ ${#probe_scripts[@]} -eq 0 ]]; then
  echo "static_probe_contract: no probe scripts found" >&2
  exit 1
fi

status=0

echo "static_probe_contract: checking ${#probe_scripts[@]} probes"

for script in "${probe_scripts[@]}"; do
  rel_path=${script#"${REPO_ROOT}/"}
  errors=()

  if [[ $(head -n1 "${script}") != "#!/usr/bin/env bash" ]]; then
    errors+=("missing #!/usr/bin/env bash shebang")
  fi

  if ! grep -q 'set -euo pipefail' "${script}"; then
    errors+=("missing 'set -euo pipefail'")
  fi

  if ! bash -n "${script}" >/dev/null 2>&1; then
    errors+=("bash -n syntax check failed")
  fi

  if ! grep -q 'bin/emit-record' "${script}"; then
    errors+=("no reference to bin/emit-record")
  fi

  probe_name=$(extract_probe_var "${script}" "probe_name" || true)
  file_stem=$(basename "${script}" .sh)
  if [[ -z "${probe_name}" ]]; then
    errors+=("probe_name is not defined")
  elif [[ "${probe_name}" != "${file_stem}" ]]; then
    errors+=("probe_name '${probe_name}' does not match filename '${file_stem}'")
  fi

  primary_capability=$(extract_probe_var "${script}" "primary_capability_id" || true)
  if [[ -z "${primary_capability}" ]]; then
    errors+=("primary_capability_id is not defined")
  fi

  if [[ ${#errors[@]} -eq 0 ]]; then
    printf '  [PASS] %s\n' "${rel_path}"
  else
    status=1
    printf '  [FAIL] %s\n' "${rel_path}"
    for err in "${errors[@]}"; do
      printf '         - %s\n' "${err}"
    done
  fi

done

exit ${status}
