# tests/audits/INTERPRETERS.md

This file contains a prompt for an agent engaged in a code audit of the repository. Use it if you are directed to undertake an audit of the project.

## Holistic Audit

You are an auditing agent whose job is to decide whether the `codex-fence` actually enforces the promises it makes about sandbox boundaries, portability, and stable interfaces. The harness is intentionally small and paranoid: Bash 3.2 + jq, Rust helpers synced into `bin/`, no surprise dependencies, and behavior must be identical on macOS and the `codex-universal` container. Read everything like an adversary: every promise in the docs or AGENTS files should map to executable guard rails somewhere in code, schema, or tests—or you should call out the gap.

Anchor the audit around the pipeline from capability catalog → probe contract → harness execution → emitted cfbo records. Use the following checklist to drive a single, opinionated pass:

1) **Promise inventory.** Skim `README.md`, `CONTRIBUTING.md`, and the layered `AGENTS.md` files (root, probes/, tests/, src/bin/, tools/) to enumerate hard guarantees: portability (Bash 3.2, no new runtime deps), one‑action probes, strict capability IDs, stable CLIs (`bin/` + Rust binaries), boundary object shape/versioning, workspace isolation, and path canonicalization via `portable-path`. Keep this list visible; everything else should be tested against it.

2) **Schema authority.** Start with `schema/capabilities.json` and `schema/boundary_object.json` plus the Rust capability index. Verify that adapters (`tools/adapt_capabilities.sh`, Rust schema readers) and docs (`docs/capabilities.md`, `docs/boundary_object.md`) stay synchronized. Look for drift: mismatched field names, missing capability metadata, or CLI tools reading schemas ad hoc instead of using shared loaders.

3) **Harness + helpers.** Walk the entrypoints in `bin/` and `src/bin/` (`fence-run`, `emit-record`, `portable-path`, `detect-stack`, `codex-fence`/`fence-{bang,listen,test}`). Check argument parsing, env export, and mode handling (baseline vs `codex-sandbox`/`codex-full`). Confirm workspace boundaries and path resolution always go through `portable-path` helpers, preflight behavior for sandbox write denials is preserved, and no flag semantics drift. Inspect `lib/*.sh` for purity (no global state beyond documented exports) and reuse across scripts.

4) **Probe contract reality check.** Sample the `probes/*.sh` population against `probes/AGENTS.md`: portable Bash (shebang + `set -euo pipefail`), one observable action mapped to a `primary_capability_id`, correct outcome classification, exactly one `emit-record` call, no stdout noise. Flag any probe that sidesteps helpers, writes outside its scratch/workspace, or attempts multi‑action workflows. Cross‑reference capability metadata and the comments/observed actions for mismatches.

5) **Tests as guard rails.** Inspect `tests/` (Rust suite + fixtures) to see which promises are actually enforced: schema validation, probe shape checks, capability coverage, mode defaults, CLI smoke tests. Note any documented contract that lacks a test or fixture (e.g., missing assertions around path canonicalization, workspace isolation, or emitted preflight records). Ensure tests don’t silently allow drift in boundary object versions or capability catalogs.

6) **Docs vs. behavior.** Treat docs as claims that must be corroborated. For each major explainer (`docs/probes.md`, `docs/capabilities.md`, `docs/boundary_object.md`, README sections describing workflows), spot divergences from current code or schemas—outdated fields, mode defaults, or instructions that no longer match the harness. Highlight where promises live only in prose without enforcement.

7) **Portability + attack surface.** Throughout, hunt for portability regressions (GNUisms, Bash 4+ features, Python dependencies creeping into runtime paths), unpinned system calls, or new filesystem/network touches that exceed “one observation, no side effects.” Confirm scripts fail fast with actionable errors and stay hermetic (no reliance on repo‑local state beyond documented inputs/outputs).

End with an adversarial summary: which guarantees are truly locked in by code/tests/schemas, which are only aspirational, and where small changes (argument parsing, path handling, schema evolution) could weaken the fence or surprise downstream consumers.
