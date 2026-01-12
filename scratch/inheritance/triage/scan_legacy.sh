#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scratch/inheritance/triage/scan_legacy.sh [options]

Scans scratch/inheritance/*/*.legacy for pre-exec hazard signals and prints a
TSV report or derived outputs (ids/patch).

Options:
  --root DIR        Corpus root (default: ./scratch/inheritance)
  --rules PATH      rules.json path (default: ./scratch/inheritance/triage/rules.json)
  --mode MODE       One of:
                     hazards  (default) TSV: script.id  hazard  match  legacy_path
                     ids      Unique script ids that matched any hazard
                     patch    Unified diff to add hazard ids to rules.json quarantine list
                     check    Exit 1 if any hazard id is not already quarantined
  -h, --help        Show this help
EOF
}

root="scratch/inheritance"
rules="scratch/inheritance/triage/rules.json"
mode="hazards"

while [[ $# -gt 0 ]]; do
  case "${1}" in
    --root) root="${2:-}"; shift 2 ;;
    --rules) rules="${2:-}"; shift 2 ;;
    --mode) mode="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "Unknown argument: ${1}" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ ! -d "${root}" ]]; then
  echo "error: not a directory: ${root}" >&2
  exit 2
fi

if [[ "${mode}" == "patch" || "${mode}" == "check" ]]; then
  if [[ ! -f "${rules}" ]]; then
    echo "error: missing rules file: ${rules}" >&2
    exit 2
  fi
fi

if ! command -v rg >/dev/null 2>&1; then
  echo "error: rg is required for scan_legacy.sh" >&2
  exit 2
fi

tmp="$(mktemp -t scan_legacy.XXXXXX)"
trap 'rm -f "${tmp}" "${tmp}.new"' EXIT

# Columns: script.id, hazard, match (line:content), legacy_path
: > "${tmp}"

hazard_keys=(
  "interactive.read_prompt"
  "privilege.sudo"
  "git.push"
  "git.tag_delete"
  "git.reset_hard"
  "git.clean_fdx"
  "timebox.sleep"
  "destructive.rm_rf"
  "destructive.disk"
  "destructive.dd"
  "network.curl_wget"
  "network.ssh_scp"
  "compat.bash4"
)
hazard_patterns=(
  "read[[:space:]].*-p[[:space:]]"
  "(^|[[:space:];])sudo([[:space:]]|$)"
  "(^|[[:space:];])git[[:space:]]+push([[:space:]]|$)"
  "(^|[[:space:];])git[[:space:]]+tag[[:space:]]+(-d|--delete)([[:space:]]|$)"
  "(^|[[:space:];])git[[:space:]]+reset[[:space:]]+--hard([[:space:]]|$)"
  "(^|[[:space:];])git[[:space:]]+clean([[:space:]]|$)[^#\\n]*-f[^#\\n]*-d[^#\\n]*-x"
  "(^|[[:space:];])sleep[[:space:]]+[0-9]+([[:space:]]|$)"
  "rm[[:space:]]+-rf"
  "(^|[[:space:];])(diskutil|fdisk|parted|mkfs)([[:space:]]|$)"
  "dd[[:space:]]+if="
  "(^|[[:space:];])(curl|wget)([[:space:]]|$)"
  "(^|[[:space:];])(ssh|scp)([[:space:]]|$)"
  "(^|[[:space:];])(local[[:space:]]+-n|declare[[:space:]]+-g|declare[[:space:]]+-[^[:space:]]*A|declare[[:space:]]+-n|mapfile|readarray)([[:space:]]|$)"
)

# Discover run dirs: immediate children with the triad.
while IFS= read -r run_dir; do
  [[ -d "${run_dir}" ]] || continue
  if [[ ! -f "${run_dir}/boundaries.json" || ! -f "${run_dir}/gates.json" || ! -f "${run_dir}/commitments.json" ]]; then
    continue
  fi

  for legacy_path in "${run_dir}"/*.legacy; do
    [[ -f "${legacy_path}" ]] || continue
    script_id="$(basename "${legacy_path}" .legacy)"

    idx=0
    while [[ "${idx}" -lt "${#hazard_keys[@]}" ]]; do
      key="${hazard_keys[$idx]}"
      pat="${hazard_patterns[$idx]}"
      match="$(rg -n --no-heading -S -m 1 -e "${pat}" "${legacy_path}" 2>/dev/null || true)"
      if [[ -n "${match}" ]]; then
        printf '%s\t%s\t%s\t%s\n' "${script_id}" "${key}" "${match}" "${legacy_path}" >> "${tmp}"
      fi
      idx=$((idx + 1))
    done
  done
done < <(find "${root}" -mindepth 1 -maxdepth 1 -type d -print | sort)

hazard_ids="$(cut -f1 "${tmp}" | sort -u | sed '/^$/d' || true)"

case "${mode}" in
  hazards)
    printf '%s\n' "script.id\thazard\tmatch\tlegacy_path"
    cat "${tmp}"
    ;;
  ids)
    printf '%s\n' "${hazard_ids}"
    ;;
  patch)
    existing="$(jq -r '.quarantine.script_ids // [] | .[]' "${rules}" | sort -u || true)"
    merged="$(printf '%s\n%s\n' "${existing}" "${hazard_ids}" | sed '/^$/d' | sort -u)"
    merged_json="$(printf '%s\n' "${merged}" | jq -R -s 'split("\n") | map(select(length>0))')"
    jq --argjson ids "${merged_json}" '.quarantine.script_ids = $ids' "${rules}" > "${tmp}.new"
    diff -u "${rules}" "${tmp}.new" || true
    ;;
  check)
    printf '%s\n' "script.id\thazard\tmatch\tlegacy_path"
    cat "${tmp}"

    existing="$(jq -r '.quarantine.script_ids // [] | .[]' "${rules}" | sort -u || true)"
    missing="$(comm -23 <(printf '%s\n' "${hazard_ids}") <(printf '%s\n' "${existing}") || true)"
    if [[ -n "${missing}" ]]; then
      echo "fail: hazards detected that are not quarantined in ${rules}:" >&2
      printf '%s\n' "${missing}" >&2
      exit 1
    fi
    ;;
  *)
    echo "error: unknown mode: ${mode}" >&2
    usage
    exit 2
    ;;
esac
