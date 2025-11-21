#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Guard-rail summary:
#   * Normalizes `schema/capabilities.json` into a predictable, jq-validated map
#     keyed by capability ID so other tooling can rely on a single canonical
#     structure.
#   * Enforces the schema version and fails fast on missing files or IDs to keep
#     downstream validation scripts from operating on stale or partial data.
# -----------------------------------------------------------------------------
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
repo_root=$(cd "${script_dir}/.." >/dev/null 2>&1 && pwd)
expected_schema_version="macOS_codex_v1"
adapter_name="adapt_capabilities"

# Allow callers to override the default schema path (useful for tests).
capabilities_file="${1:-${repo_root}/schema/capabilities.json}"

if [[ ! -f "${capabilities_file}" ]]; then
  echo "${adapter_name}: unable to find capabilities.json at ${capabilities_file}" >&2
  exit 1
fi

# Keep the jq program inline so the adapter remains hermetic and easy to audit.
read -r -d '' jq_program <<'JQ' || true
def to_object:
  if type == "object" then
    .
  else
    {}
  end;

def to_array:
  if . == null then
    []
  elif type == "array" then
    map(select(. != null))
  else
    [.] | map(select(. != null))
  end;

def normalize_sources:
  to_array
  | map({
      doc: .doc,
      section: (.section // null),
      url_hint: (.url_hint // null)
    } | with_entries(select(.value != null)));

if (.schema_version | type) != "string" then
  error("adapt_capabilities: expected schema_version to be a string, got \(.schema_version|type)")
elif (.schema_version | test("^[A-Za-z0-9_.-]+$") | not) then
  error("adapt_capabilities: schema_version must match ^[A-Za-z0-9_.-]+$, got \(.schema_version)")
elif .schema_version != $expected_version then
  error("adapt_capabilities: expected schema_version=\($expected_version), got \(.schema_version)")
else
  (.capabilities // [])
  | reduce .[] as $cap (
      {};
      ($cap.operations | to_object) as $ops |
      if ($cap.id // "") == "" then
        error("adapt_capabilities: encountered capability with no id")
      elif has($cap.id) then
        error("adapt_capabilities: duplicate capability id \($cap.id)")
      else
        .[$cap.id] = {
          id: $cap.id,
          category: ($cap.category // null),
          layer: ($cap.layer // null),
          description: ($cap.description // null),
          notes: ($cap.notes // null),
          operations: {
            allow: ($ops.allow | to_array),
            deny: ($ops.deny | to_array)
          },
          meta_ops: ($cap.meta_ops | to_array),
          agent_controls: ($cap.agent_controls | to_array),
          sources: ($cap.sources | normalize_sources)
        }
      end
    )
end
JQ

jq -e -S --arg expected_version "${expected_schema_version}" "${jq_program}" "${capabilities_file}"
