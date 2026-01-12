#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scratch/inheritance/triage/triage.sh

One turn = one timestamped directory with:
  pre/{all.ndjson,all.stderr,items.tsv,classes.tsv}
  post/{all.ndjson,all.stderr,items.tsv,classes.tsv}

Flow:
  run_all (pre) -> report (pre) -> gen_wrappers -> run_all (post) -> report (post) -> gates
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"

turn_id="$(date -u +"%Y%m%dT%H%M%SZ")"
turn_dir="${repo_root}/scratch/inheritance/triage/turns/${turn_id}"

mkdir -p "${turn_dir}/pre" "${turn_dir}/post"

# Pre-exec hazard scan: enforce quarantine list before running anything.
if ! "${script_dir}/scan_legacy.sh" \
  --root "${repo_root}/scratch/inheritance" \
  --rules "${repo_root}/scratch/inheritance/triage/rules.json" \
  --mode check \
  > "${turn_dir}/pre/hazards.tsv"; then
  "${script_dir}/scan_legacy.sh" \
    --root "${repo_root}/scratch/inheritance" \
    --rules "${repo_root}/scratch/inheritance/triage/rules.json" \
    --mode patch \
    > "${turn_dir}/pre/quarantine.patch" || true
  echo "hazard scan output: ${turn_dir}/pre/hazards.tsv" >&2
  echo "suggested quarantine patch: ${turn_dir}/pre/quarantine.patch" >&2
  exit 1
fi

"${script_dir}/run_all.sh" --out-dir "${turn_dir}/pre"
"${script_dir}/report.sh" "${turn_dir}/pre"

"${script_dir}/gen_wrappers.sh"

"${script_dir}/run_all.sh" --out-dir "${turn_dir}/post"
"${script_dir}/report.sh" "${turn_dir}/post"

# Gates: fail fast on backsliding.
"${repo_root}/scratch/inheritance/report-bash4-exit0.sh" "${turn_dir}/post/all.ndjson"
"${script_dir}/check_no_synthetic.sh" "${turn_dir}/post/all.ndjson"
"${script_dir}/check_no_self_ref.sh" "${repo_root}/scratch/inheritance"
"${script_dir}/check_commitments_declared.sh" "${turn_dir}/post/all.ndjson" "${turn_dir}/post/script_map.json"
"${script_dir}/check_quarantine_respected.sh" "${turn_dir}/post/all.ndjson" "${repo_root}/scratch/inheritance/triage/rules.json"

echo "turn_dir: ${turn_dir}"
