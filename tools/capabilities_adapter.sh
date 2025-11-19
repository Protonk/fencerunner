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

# Allow callers to override the default schema path (useful for tests).
capabilities_file="${1:-${repo_root}/schema/capabilities.json}"

if [[ ! -f "${capabilities_file}" ]]; then
  echo "capabilities_adapter: unable to find capabilities.json at ${capabilities_file}" >&2
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

if .schema_version != 3 then
  error("capabilities_adapter: expected schema_version=3, got \(.schema_version)")
else
  (.capabilities // [])
  | reduce .[] as $cap (
      {};
      ($cap.operations | to_object) as $ops |
      if ($cap.id // "") == "" then
        error("capabilities_adapter: encountered capability with no id")
      else
        .[$cap.id] = {
          id: $cap.id,
          category: ($cap.category // null),
          layer: ($cap.layer // null),
          status: ($cap.status // null),
          level: ($cap.level // null),
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

jq -e -S "${jq_program}" "${capabilities_file}"
