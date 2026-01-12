#!/usr/bin/env bash
set -euo pipefail

ndjson="${1:-}"
rules="${2:-scratch/inheritance/triage/rules.json}"

if [[ -z "${ndjson}" ]]; then
  echo "usage: scratch/inheritance/triage/check_quarantine_respected.sh <all.ndjson> [rules.json]" >&2
  exit 2
fi

if [[ ! -f "${ndjson}" ]]; then
  echo "error: not a file: ${ndjson}" >&2
  exit 2
fi
if [[ ! -f "${rules}" ]]; then
  echo "error: not a file: ${rules}" >&2
  exit 2
fi

quarantine_ids_json="$(jq -c '.quarantine.script_ids // []' "${rules}")"

violations="$(
  jq -r --argjson q "${quarantine_ids_json}" '
    select(.script.id as $id | ($q | index($id)) != null)
    | select(.operation.kind == "legacy.exec")
    | .script.id
  ' "${ndjson}" | sort -u
)"

if [[ -n "${violations}" ]]; then
  echo "fail: quarantined scripts emitted legacy.exec:" >&2
  printf '%s\n' "${violations}" >&2
  exit 1
fi

