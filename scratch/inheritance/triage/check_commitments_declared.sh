#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scratch/inheritance/triage/check_commitments_declared.sh <ALL_NDJSON> <SCRIPT_MAP_JSON>

Fails if any boundary record contains a commitment id that is not declared in the
origin run dir's commitments.json registry.
EOF
}

ndjson="${1:-}"
script_map_json="${2:-}"

if [[ -z "${ndjson}" || -z "${script_map_json}" || "${ndjson}" == "-h" || "${ndjson}" == "--help" ]]; then
  usage
  exit 2
fi

if [[ ! -f "${ndjson}" ]]; then
  echo "error: not a file: ${ndjson}" >&2
  exit 2
fi

if [[ ! -f "${script_map_json}" ]]; then
  echo "error: not a file: ${script_map_json}" >&2
  exit 2
fi

tmp_map="$(mktemp -t commitments_declared.map.XXXXXX)"
trap 'rm -f "${tmp_map}"' EXIT

jq -r 'to_entries[] | "\(.key)\t\(.value.run_dir)"' "${script_map_json}" > "${tmp_map}"

fail=0
while IFS=$'\t' read -r script_id commitments_csv; do
  [[ -n "${script_id}" ]] || continue

  run_dir="$(awk -F '\t' -v id="${script_id}" '$1==id{print $2; exit}' "${tmp_map}")"
  if [[ -z "${run_dir}" ]]; then
    echo "fail: unknown run_dir for script.id=${script_id}" >&2
    fail=1
    continue
  fi

  commitments_path="${run_dir}/commitments.json"
  if [[ ! -f "${commitments_path}" ]]; then
    echo "fail: missing commitments.json for ${script_id}: ${commitments_path}" >&2
    fail=1
    continue
  fi

  declared_ids="$(jq -r '.commitments[]?.id' "${commitments_path}" | sort -u)"

  if [[ -z "${commitments_csv}" ]]; then
    continue
  fi

  IFS=',' read -r -a record_ids <<< "${commitments_csv}"
  for cid in "${record_ids[@]}"; do
    [[ -n "${cid}" ]] || continue
    if ! printf '%s\n' "${declared_ids}" | grep -Fxq "${cid}"; then
      echo "fail: ${script_id} emitted undeclared commitment id=${cid} (registry: ${commitments_path})" >&2
      fail=1
    fi
  done
done < <(
  jq -r '
    .script.id as $id
    | [
        $id,
        ((.context.commitments // []) | map(.id) | join(","))
      ]
    | @tsv
  ' "${ndjson}"
)

if [[ "${fail}" -ne 0 ]]; then
  exit 1
fi

