# Capabilities catalog

`spec/capabilities.yaml` is the catalog of sandbox and agent behaviors that codex-fence cares about. It defines the vocabulary probes use to describe what they exercised and lets downstream tooling understand how those behaviors map to Seatbelt rules or Codex agent policy. This file is deliberately scoped: it is **not** a full Seatbelt reference, only the subset that codex-fence currently probes and reasons about.

The current schema version is **2**, focused on macOS Seatbelt behavior plus the Codex agent policy that orchestrates it. The structure is designed to grow as we add more platforms or classes of capabilities.

## Scope and supporting docs

The top-level `scope` block captures high-level metadata:

- `description` summarizes what this catalog represents.
- `platforms` lists the platforms covered so far (currently `[macos]`, but intentionally expandable).
- `notes` describes the modeling boundaries and future direction (more platforms, incremental growth).

The `docs` map collects canonical references. Each key (e.g. `apple_sandbox_guide`, `deep_dive_agent_sandboxes`) acts as a stable identifier for an external source. Capability entries refer to these identifiers via `sources[*].doc`, so URLs only live in one place. Updating a reference happens under `docs`, keeping per-capability entries short and consistent.

## Capability schema

Each entry under `capabilities` follows this schema:

- `id` – stable identifier probes and boundary objects use. Never rename without updating every consumer.
- `category` – high-level grouping (`filesystem`, `process`, `network`, `sysctl`, `ipc`, `sandbox_meta`, `agent_policy`).
- `platform` – list of platforms where the definition applies (currently `[macos]`, ready for more).
- `layer` – conceptual layer:
  - `os_sandbox` for Seatbelt/system enforcement and profile shape.
  - `agent_policy` for Codex orchestration (approvals, sandbox toggles).
  - `sandbox_meta` for profile mechanics (params, debug).
- `status` – `core | experimental | planned`; use `experimental` until a probe proves the behavior.
- `description` – one-sentence summary of what the capability covers.
- `operations` – nested structure describing primitive Seatbelt operations:
  - `allow: [...]` enumerates SBPL primitives the capability depends on (e.g., `file-read*`, `mach-lookup`).
  - `deny: [...]` lists primitives that must be blocked; often empty but recorded when denial is the core behavior.
  These values are primitive operations, not profile keywords.
- `meta_ops` – policy-mechanism tags (e.g., `sandbox-meta:default-deny`, `sandbox-meta:parametric-profile`) that describe how profiles are built or instrumented.
- `agent_controls` – agent-level control tags (`agent-policy:*`) describing approvals modes, trust lists, or other orchestration knobs.
- `level` – coarse impact/importance (`low`, `medium`, `high`).
- `notes` – free-form guidance for probe authors (how to trigger the behavior, caveats).
- `sources` – provenance list; each entry is `{doc, section, url_hint?}` referencing a key in `docs`. `section` is a human hint, `url_hint` is optional and can reference fragments or filenames.

## Categories and layers

- `filesystem` covers workspace roots, .git guards, user/system dirs, symlink behavior, and similar file access.
- `process` captures exec/fork semantics and how tools spawn child processes.
- `network` describes outbound network policy or loopback allowances.
- `sysctl` represents kernel parameter reads (e.g., `sysctl -n hw.ncpu`).
- `ipc` overlaps with Mach services and other inter-process messaging.
- `sandbox_meta` focuses on how the sandbox profile is structured (default deny, param passing, debug logging).
- `agent_policy` covers Codex-level controls: approvals modes, default sandboxing, command trust lists.

`layer` clarifies where the capability “lives”:

- `os_sandbox` – enforced by Seatbelt/kernel (syscalls, profile entries).
- `sandbox_meta` – properties of the profile itself, such as using `(deny default)` or emitting debug traces.
- `agent_policy` – choices the Codex agent makes before/after running commands, independent of macOS internals.

## Using the capabilities catalog

- Probes must reference capability `id`s directly; never invent one without adding the corresponding entry.
- Multi-capability probes are allowed: “one thing per probe” refers to the behavior under test, not the number of capability IDs mentioned.
- When adding a capability:
  - Deliberately choose `category`, `platform`, and `layer`.
  - Start with `status: experimental` until at least one reliable probe exists.
  - Add at least one `source` referencing the relevant doc section.
- When writing docs, prompts, or guidance, anchor statements to capability IDs instead of re-describing the sandbox from scratch.

## Keep it tied to code

`spec/capabilities.yaml` remains the source of truth; this Markdown file explains how to interpret and use it. If the schema changes (new fields, `layer` values, `status` options), update `docs/capabilities.md` in the same change. If a capability is renamed or removed, update probes, boundary-object logic, and this catalog together so the ecosystem stays in sync.
