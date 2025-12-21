# Capability Catalog Guide 

This document summarizes the structure of the capability catalog schema and the bundled catalog instance so humans and agents can quickly understand how capabilities are represented.

> NOTE: This document exists to help agents understand the structure of the catalog, NOT to provide additional technical or policy information.

The catalog schema lives at `schema/capability_catalog.schema.json` (current schema version: **sandbox_catalog_v1**). The bundled catalog instance is stored at `catalogs/macos_codex_v1.json` (catalog key: **macOS_codex_v1**) and can be swapped for another catalog without code changes. CLI/env overrides (`--catalog` / `CATALOG_PATH`) select alternates; otherwise the bundled catalog is used.

## Catalog keys and repositories

- `schema_version` is the catalog schema version (currently `sandbox_catalog_v1`).
- `catalog.key` is the **CatalogKey** used across the harness. Boundary objects echo it as `capabilities_schema_version` so downstream readers know which catalog to consult.
- The Rust types under `src/catalog/` load a catalog JSON into a `CapabilityCatalog`, validate it via `CapabilityIndex`, and can optionally register it inside a `CatalogRepository`. The repository is intentionally generic—drop in another catalog JSON with a different `catalog.key`, register it, and the same lookup helpers work. Use `--catalog`/`CATALOG_PATH` to point helpers at a different file; new catalogs must match the canonical schema version.
- Probes stay insulated from catalog internals: they declare `CapabilityId`s, while the harness resolves those IDs to `CapabilitySnapshot`s when emitting boundary-event records. Guard-rail tests in `tests/schema.rs` and `tests/catalog.rs` (for example, `capability_catalog_schema`, `load_real_catalog_smoke`, and `repository_lookup_context_matches_capabilities`) ensure that catalogs, snapshots, and boundary objects stay in sync.

## Catalog structure

Top-level fields:
- `schema_version` — catalog schema version (`sandbox_catalog_v1`).
- `catalog` — metadata about the catalog instance (`key`, `title`, optional `description`/`labels`/`notes`).
- `scope` — description of what the catalog covers plus `policy_layers`, `categories`, and optional `limitations`/`notes`.
- `docs` — bibliography map used by capability sources.
- `capabilities` — array of capability entries.

Policy and category snapshot (bundled catalog):
- 2 policy layers (`os_sandbox`, `agent_runtime`).
- 7 categories (`filesystem`, `process`, `network`, `sysctl`, `ipc`, `sandbox_profile`, `agent_sandbox_policy`).
- 22 capabilities total in the example macOS catalog (other catalogs may define different counts as long as they satisfy the schema and index checks).

## Capability entries

Each entry records one observable behavior or constraint.

Fields:
- `id` — stable slug, e.g., `cap_fs_write_workspace_tree`.
- `category` / `layer` — must match an entry in `scope.categories` / `scope.policy_layers`.
- `description` — concise, user-facing summary.
- `status` — optional free-form assessment label.
- `operations` — `{allow: [...], deny: [...]}` lists of relevant low-level operations.
- `meta_ops` — `sandbox-meta:*` tags describing profile techniques.
- `agent_controls` — `agent-policy:*` tags describing agent-level knobs.
- `labels` — optional string tags to mark environment or client-specific bits.
- `notes` — probe-author hints or contextual commentary.
- `sources` — citations `{doc, section?, url_hint?}` pointing back to `docs`.

## Example entry

Excerpt from `catalogs/macos_codex_v1.json`:

```json
{
  "id": "cap_fs_write_workspace_tree",
  "category": "filesystem",
  "layer": "os_sandbox",
  "description": "Ability for commands to create/modify/delete files inside the workspace roots.",
  "operations": {
    "allow": [
      "file-write*",
      "file-write-data"
    ],
    "deny": []
  },
  "meta_ops": [],
  "agent_controls": [],
  "labels": ["macos"],
  "notes": "Codex enumerates a set of writable roots from SandboxPolicy and passes them as WRITABLE_ROOT_i parameters into the Seatbelt profile; writes are allowed only under these roots, with everything else denied by default. Probes should verify both creation and deletion semantics inside a known workspace subdirectory.\n",
  "sources": [
    {"doc": "apple_sandbox_guide", "section": "5.2 - file-write* family"},
    {"doc": "run_code_sandbox", "section": "Deny-by-default Python sandbox with explicit file-read* and file-write* rules"},
    {"doc": "deep_dive_agent_sandboxes", "section": "Seatbelt WRITABLE_ROOT_i construction and writable_folder_policies"}
  ]
}
```

As the catalog evolves, keep the schema and tests aligned so probe authors and downstream consumers can trust capability IDs and metadata.
