#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
# shellcheck source=tests/lib/utils.sh
source "${script_dir}/lib/utils.sh"

cd "${REPO_ROOT}"

shopt -s nullglob
probe_scripts=(probes/*.sh)
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
