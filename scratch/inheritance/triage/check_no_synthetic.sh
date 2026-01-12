#!/usr/bin/env bash
set -euo pipefail

ndjson="${1:-}"
if [[ -z "${ndjson}" ]]; then
  echo "usage: scratch/inheritance/triage/check_no_synthetic.sh <all.ndjson>" >&2
  exit 2
fi

if [[ ! -f "${ndjson}" ]]; then
  echo "error: not a file: ${ndjson}" >&2
  exit 2
fi

count="$(jq -n '
  [inputs | select(.extensions.synthetic? != null)] | length
' "${ndjson}")"

if [[ "${count}" -ne 0 ]]; then
  echo "fail: synthetic records remain: ${count}" >&2
  jq -r '
    select(.extensions.synthetic? != null)
    | [.script.id, .operation.kind, .result.outcome, (.result.details.message // "")] | @tsv
  ' "${ndjson}" | sed -n '1,50p' >&2
  exit 1
fi

