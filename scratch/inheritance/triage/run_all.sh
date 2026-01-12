#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scratch/inheritance/triage/run_all.sh [options]

Runs the entire scratch/inheritance corpus under fencerunner (--supervised) and
writes a canonical per-turn artifact pair:
  all.ndjson
  all.stderr

Options:
  --out-dir DIR        Write artifacts into DIR (no timestamping).
  --out-root DIR       Parent dir for timestamped output (default: ./scratch/inheritance/triage/turns).
  --turn-id ID         Turn id (default: UTC timestamp).
  --fencerunner PATH   Path to fencerunner binary (default: ./target/debug/fencerunner).
  -h, --help           Show this help text.
EOF
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"

out_dir=""
out_root="${repo_root}/scratch/inheritance/triage/turns"
turn_id=""
fencerunner_bin="${repo_root}/target/debug/fencerunner"

while [[ $# -gt 0 ]]; do
  case "${1}" in
    --out-dir)
      out_dir="${2:-}"; shift 2 ;;
    --out-root)
      out_root="${2:-}"; shift 2 ;;
    --turn-id)
      turn_id="${2:-}"; shift 2 ;;
    --fencerunner)
      fencerunner_bin="${2:-}"; shift 2 ;;
    -h|--help)
      usage; exit 0 ;;
    *)
      echo "Unknown argument: ${1}" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ -z "${turn_id}" ]]; then
  turn_id="$(date -u +"%Y%m%dT%H%M%SZ")"
fi

if [[ -z "${out_dir}" ]]; then
  out_dir="${out_root}/${turn_id}"
  if [[ -e "${out_dir}" ]]; then
    suffix=1
    while [[ -e "${out_dir}-${suffix}" ]]; do
      suffix=$((suffix + 1))
    done
    out_dir="${out_dir}-${suffix}"
  fi
fi

mkdir -p "${out_dir}"

if [[ ! -x "${fencerunner_bin}" ]]; then
  echo "error: fencerunner not executable: ${fencerunner_bin}" >&2
  exit 2
fi

# Discover run dirs: immediate children of scratch/inheritance that contain the triad.
run_dirs=()
while IFS= read -r candidate; do
  if [[ -f "${candidate}/boundaries.json" && -f "${candidate}/gates.json" && -f "${candidate}/commitments.json" ]]; then
    run_dirs+=("${candidate}")
  fi
done < <(find "${repo_root}/scratch/inheritance" -mindepth 1 -maxdepth 1 -type d -print | sort)

if [[ "${#run_dirs[@]}" -eq 0 ]]; then
  echo "error: no run dirs found under ${repo_root}/scratch/inheritance" >&2
  exit 1
fi

printf '%s\n' "${run_dirs[@]}" > "${out_dir}/run_dirs.txt"

set +e
"${fencerunner_bin}" --supervised "${run_dirs[@]}" > "${out_dir}/all.ndjson" 2> "${out_dir}/all.stderr"
exit_code="$?"
set -e

printf '%s\n' "${exit_code}" > "${out_dir}/exit_code.txt"

printf '%s\n' "${out_dir}"
