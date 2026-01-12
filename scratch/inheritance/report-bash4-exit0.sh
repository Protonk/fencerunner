#!/usr/bin/env bash
set -euo pipefail

root_or_ndjson="${1:-scratch/inheritance}"

if [[ -f "${root_or_ndjson}" ]]; then
  ndjson="${root_or_ndjson}"

  re='local: -n: invalid option|declare: -g: invalid option|declare: -A: invalid option'
  matches="$(
    jq -r --arg re "${re}" '
      select(.result.outcome == "success")
      | select((.result.details.exit_code // 0) == 0)
      | select(.payload.stderr_snippet | test($re))
      | [.script.id, (.payload.stderr_snippet | split("\n")[0])] | @tsv
    ' "${ndjson}" | sort -u
  )"
  if [[ -n "${matches}" ]]; then
    echo "fail: success+exit0 but bash>=4 evidence present:" >&2
    printf '%s\n' "${matches}" >&2
    exit 1
  fi
  exit 0
fi

root="${root_or_ndjson}"
if [[ ! -d "${root}" ]]; then
  echo "usage: $0 [ROOT_DIR|ALL_NDJSON]" >&2
  echo "error: not a directory or ndjson file: ${root_or_ndjson}" >&2
  exit 2
fi

re='local: -n: invalid option|declare: -g: invalid option|declare: -A: invalid option'

found=0
for dir in "${root}"/*; do
  [[ -d "${dir}" ]] || continue

  latest="$(ls "${dir}"/stream*.ndjson 2>/dev/null | sort -V | tail -n 1 || true)"
  [[ -n "${latest}" ]] || continue

  out="$(
    jq -r --arg dir "${dir}" --arg re "${re}" '
      select(.result.outcome == "success")
      | select((.result.details.exit_code // 0) == 0)
      | select(.payload.stderr_snippet | test($re))
      | [$dir, .script.id, (.payload.stderr_snippet | split("\n")[0])] | @tsv
    ' "${latest}"
  )"
  if [[ -n "${out}" ]]; then
    printf '%s\n' "${out}"
    found=1
  fi
done

if [[ "${found}" -ne 0 ]]; then
  exit 1
fi
