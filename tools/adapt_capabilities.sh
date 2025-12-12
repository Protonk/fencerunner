#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# Guard-rail summary:
#   * Normalizes the bundled capability catalog into a predictable map keyed by
#     capability ID so other tooling can rely on a single canonical structure.
#   * Enforces the catalog schema version and fails fast on missing files or IDs
#     to keep downstream validation scripts from operating on stale or partial
#     data.
# -----------------------------------------------------------------------------
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
repo_root=$(cd "${script_dir}/.." >/dev/null 2>&1 && pwd)
expected_schema_version="sandbox_catalog_v1"
adapter_name="adapt_capabilities"
defaults_manifest="${repo_root}/catalogs/defaults.json"
json_extract_bin="${repo_root}/bin/json-extract"

resolve_default_descriptor_path() {
  local key="$1"
  local fallback="$2"
  local value=""
  if [[ -f "${defaults_manifest}" && -x "${json_extract_bin}" ]]; then
    value=$("${json_extract_bin}" --file "${defaults_manifest}" --pointer "/${key}" --type string --default "" 2>/dev/null || true)
  fi
  if [[ -n "${value}" ]]; then
    value="${value#./}"
    if [[ "${value}" == /* ]]; then
      printf '%s\n' "${value}"
    else
      printf '%s\n' "${repo_root}/${value}"
    fi
  else
    printf '%s\n' "${fallback}"
  fi
}

if [[ ! -x "${json_extract_bin}" ]]; then
  echo "${adapter_name}: missing json-extract helper at ${json_extract_bin}" >&2
  exit 1
fi

# Allow callers to override the default catalog path (useful for tests).
capabilities_file="${1:-$(resolve_default_descriptor_path "catalog" "${repo_root}/catalogs/macos_codex_v1.json")}"

if [[ ! -f "${capabilities_file}" ]]; then
  echo "${adapter_name}: unable to find capabilities.json at ${capabilities_file}" >&2
  exit 1
fi

schema_version=$("${json_extract_bin}" --file "${capabilities_file}" --pointer "/schema_version" --type string --default "" 2>/dev/null || true)
if [[ -z "${schema_version}" ]]; then
  echo "${adapter_name}: expected schema_version to be a string" >&2
  exit 1
fi
if [[ ! "${schema_version}" =~ ^[A-Za-z0-9_.-]+$ ]]; then
  echo "${adapter_name}: schema_version must match ^[A-Za-z0-9_.-]+$, got ${schema_version}" >&2
  exit 1
fi
if [[ "${schema_version}" != "${expected_schema_version}" ]]; then
  echo "${adapter_name}: expected schema_version=${expected_schema_version}, got ${schema_version}" >&2
  exit 1
fi

catalog_key=$("${json_extract_bin}" --file "${capabilities_file}" --pointer "/catalog/key" --type string --default "" 2>/dev/null || true)
if [[ -z "${catalog_key}" ]]; then
  echo "${adapter_name}: expected catalog.key to be a non-empty string" >&2
  exit 1
fi
if [[ ! "${catalog_key}" =~ ^[A-Za-z0-9_.-]+$ ]]; then
  echo "${adapter_name}: catalog.key must match ^[A-Za-z0-9_.-]+$, got ${catalog_key}" >&2
  exit 1
fi

capabilities_json=$("${json_extract_bin}" --file "${capabilities_file}" --pointer "/capabilities" --type array --default "[]" 2>/dev/null) || {
  echo "${adapter_name}: failed to read capabilities array" >&2
  exit 1
}

python3 - "$capabilities_json" <<'PY'
import json, sys

caps = json.loads(sys.argv[1])
if not isinstance(caps, list):
    sys.stderr.write("adapt_capabilities: capabilities must be an array\n")
    sys.exit(1)

result = {}
for cap in caps:
    if not isinstance(cap, dict):
        sys.stderr.write("adapt_capabilities: capability entry must be an object\n")
        sys.exit(1)
    cid = cap.get("id")
    if not cid or not isinstance(cid, str):
        sys.stderr.write("adapt_capabilities: encountered capability with no id\n")
        sys.exit(1)
    if cid in result:
        sys.stderr.write(f"adapt_capabilities: duplicate capability id {cid}\n")
        sys.exit(1)

    def normalize(val, default):
        return val if val is not None else default

    result[cid] = {
        "id": cid,
        "category": normalize(cap.get("category"), None),
        "layer": normalize(cap.get("layer"), None),
        "description": normalize(cap.get("description"), None),
        "status": normalize(cap.get("status"), None),
        "operations": normalize(cap.get("operations"), {}),
        "meta_ops": normalize(cap.get("meta_ops"), []),
        "agent_controls": normalize(cap.get("agent_controls"), []),
        "sources": normalize(cap.get("sources"), []),
        "labels": normalize(cap.get("labels"), []),
    }

print(json.dumps(result, sort_keys=True))
PY
