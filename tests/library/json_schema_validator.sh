#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Minimal JSON Schema validator wrapper that uses jq to enforce the cfbo-v1
# subset (types, const/enum, required props, additionalProperties, arrays,
# uniqueItems, and $ref). Keeps the harness dependency tree to Bash + jq.
# -----------------------------------------------------------------------------
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: json_schema_validator.sh <schema_path> <json_path>" >&2
  exit 1
fi

schema_path="$1"
json_path="$2"

if [[ ! -f "${schema_path}" ]]; then
  echo "Schema file not found: ${schema_path}" >&2
  exit 1
fi

if [[ ! -f "${json_path}" ]]; then
  echo "JSON document not found: ${json_path}" >&2
  exit 1
fi

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
jq_filter="${script_dir}/json_schema_validator.jq"

if [[ ! -f "${jq_filter}" ]]; then
  echo "jq validator filter missing: ${jq_filter}" >&2
  exit 1
fi

errors=$(jq -n \
  --slurpfile schema "${schema_path}" \
  --slurpfile instance "${json_path}" \
  -f "${jq_filter}")

if [[ "${errors}" != "[]" ]]; then
  printf '%s\n' "${errors}" | jq -r '.[] | "schema validation error: \(.)"' >&2
  exit 1
fi
