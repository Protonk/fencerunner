# Capabilities Catalog guide

This document is a brief guide to `spec/capabilities.yaml`, a structured listing of sandbox restrictions that codex-fence mdoels. It exists to help agents understand the structure of the catalog, NOT to provide additional technical or policy information--It is not a Seatbelt encyclopedia—just the schema explainer for the catalog codex-fence currently ships with.

Read this document to see how each field is intended to be used, how it maps to probes, and what supporting material belongs with every capability.  

## Catalog scope and shared references

The current schema version is **2**, centered on macOS Seatbelt plus Codex agent policy. The structure anticipates future platforms and capability classes, so every field description below should be interpreted with that growth path in mind.

- The `scope` block sets the boundary for the catalog:
  - `description` summarizes what this slice of Seatbelt/agent behavior covers.
  - `platforms` lists the platforms currently modeled (today `[macos]`, tomorrow more).
  - `notes` explains how we expect the catalog to grow (new profiles, gradual expansion).
- The `docs` map is the canonical bibliography. Each key (e.g., `apple_sandbox_guide`, `deep_dive_agent_sandboxes`) acts as a stable handle. Capabilities reference these keys in `sources[*].doc`.

## Capability entry schema

Capability entries are a record of what we know about how the stack can mediate an action. Each entry is structured data about known features of the security policy surface with the following fields described below:
- `id` — a short, snake cased slug such as `cap_fs_write_workspace_tree` which serves as a unique, stable identifier for the capability. Never rename without migrating every consumer. 

### High-level categorization

- `platform` — list of operating systems the capability statement applies to. 
- `layer` — tag noting **where** the rule lives:
  - `os_sandbox` for Seatbelt/kernel policy.
  - `sandbox_meta` for profile-construction mechanics (default deny, parameterization, logging).
  - `agent_policy` for Codex orchestration (approval prompts, sandbox toggles).
- `category` — bucket (`filesystem`, `process`, `network`, `sysctl`, `ipc`, `sandbox_meta`, `agent_policy`) for what the rule does. Capabilities listed in the catalog mediate mostly non-overlapping interactions; a call to the file system is distinct from reading `kern.boottime`. `category` should be the primary domain of the mediation.
  - `filesystem` — workspace roots, `.git` isolation, user/system directories, symlink handling, other file I/O rules.
  - `process` — exec/fork semantics, helper tools, and child-process policy.
  - `network` — outbound connectivity, loopback allowances, or explicit denials.
  - `sysctl` — kernel parameter reads such as `sysctl -n hw.ncpu`.
  - `ipc` — Mach services and other inter-process messaging.
  - `sandbox_meta` — mechanics of the sandbox profile itself.
  - `agent_policy` — Codex-level coordination outside the kernel.


### Behavioral detail

- `description` — concise, user-facing summary of what the capability defends or permits.
- `operations` — `{allow: [...], deny: [...]}` lists of SBPL primitives required for the capability. These are raw Seatbelt operations (e.g., `file-read*`, `mach-lookup`), not policy keywords, and should only include the primitives that matter for the described behavior.
- `meta_ops` — `sandbox-meta:*` tags that describe the profile techniques in play (default deny, argument templating, debug injectors, etc.).
- `agent_controls` — `agent-policy:*` tags describing agent-level knobs such as trust lists or approval requirements.


### Enforcement context and lifecycle


- `status` — `planned`, `experimental`, or `core`. Start every new entry at `experimental` until we have a reliable probe.
- `level` — fast severity/impact signal (`low`, `medium`, `high`).


### Guidance and provenance

Catalog entries are not exhausted by the above information. Useful information is held in free text in `notes`--and sources contain pointers to where we learned about the behavior. 
- `notes` — probe-author hints: how to trigger the behavior, known tricky paths, or anything we learned by testing it.
- `sources` — list of `{doc, section, url_hint?}` objects pointing back to entries in the `docs` map. Include at least one reference for every capability so downstream consumers know where our understanding of the feature came from.

## Working with spec/capabilities.yaml

- Probes **must** cite `id` values that already exist in the spec. If you need a new capability, add it to `spec/capabilities.yaml` in the same change that introduces the probe.
- “One probe per behavior” still allows multiple capability IDs when necessary; just ensure the payload makes that clear.
- Schema edits (new fields, enum values, or remapped layers) require synchronized updates to:
  1. `spec/capabilities.yaml`.
  2. This guide (`spec/AGENTS.md`) so future authors understand the field.
  3. Any logic that parses or emits capability metadata (probes, emit-record helpers, docs).
- When writing higher-level docs or prompts, link to capability IDs instead of describing rules ad hoc; that keeps the catalog authoritative.
