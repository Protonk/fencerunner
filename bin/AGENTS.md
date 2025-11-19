# Agent Guidance for Core Bin Scripts

The scripts in this directory orchestrate how probes run, how runtime metadata
is collected, and how CFBO records are emitted. They are chained together from
multiple entry points (CI jobs, CLI tools, and other agents), so edits must
optimize for stability and clarity.

## `emit-record`
- **Purpose:** Validate probe input, resolve capability metadata, capture the
  current stack info, and output a single JSON record that matches the CFBO
  schema.
- **Operational notes:** Reads `schema/capabilities.json` via
  `tools/capabilities_adapter.sh`, shells out to `bin/detect-stack`, and emits
  JSON via `jq`. It assumes no network access and must complete quickly because
  probes invoke it on every operation.
- **Expectations for edits:**
  - Keep the CLI flags backwards compatible; third-party probes shell out to
    this script exactly as documented.
  - Validate inputs eagerly with friendly errors—agents rely on actionable
    failures when wiring up new probes.
  - Do not add stateful side effects or logging beyond stderr errors; the only
    stdout output should remain the final JSON document.

## `detect-stack`
- **Purpose:** Report details about the current environment (codex CLI version,
  sandbox mode, container tag, OS, etc.) so `emit-record` can embed the context.
- **Operational notes:** Must remain dependency-free and fast because it runs
  before every record emission. The JSON shape is contractually consumed by
  downstream services.
- **Expectations for edits:**
  - Avoid adding expensive system inspection or network access; the script
    should finish in milliseconds.
  - Never change or remove existing keys without versioning—new consumers expect
    backwards compatibility.
  - When adding fields, gate them with feature detection and default to `null`
    so older environments still work.

## `fence-run`
- **Purpose:** Resolve probe scripts and execute them under one of the supported
  sandbox modes (`baseline`, `codex-sandbox`, `codex-full`), exporting context
  via environment variables used by the other scripts.
- **Operational notes:** Finds probes relative to `probes/`, checks for
  executability, and optionally shells into the codex CLI to enforce a sandbox.
- **Expectations for edits:**
  - Preserve the `fence-run MODE PROBE_NAME` interface; automation depends on
    the positional arguments.
  - Keep probe resolution strict: refuse scripts outside `probes/` and prefer
    descriptive error messages when the file cannot be run.
  - When touching sandbox integrations, make sure `FENCE_RUN_MODE` and
    `FENCE_SANDBOX_MODE` continue to reflect the caller's intent so downstream
    scripts stay informed.

## `codex-fence` + Rust helpers
- **Purpose:** Top-level CLI for `--bang`/`--listen`/`--test` that hands off to
  Rust binaries under `target/`. Hooks into the existing harness instead of
  duplicating probe logic.
- **Operational notes:** The Bash shims locate the compiled binaries next to
  the repo or on `PATH` and export `CODEX_FENCE_ROOT` so the helpers can find
  probes/tests.
- **Expectations for edits:**
  - Keep these wrappers thin and defensive; prefer fixing the Rust helpers
    rather than piling logic into Bash.
  - Preserve the data flow: `--bang` produces cfbo-v1 JSON, `--listen` consumes
    it, and `--test` delegates to `tests/run.sh`.

## General expectations for agents
- Prefer explicit, defensive checks that fail fast over implicit Bash behavior.
- Document intent with comments when control flow might surprise a future
  reader—especially around argument parsing, sandbox decisions, and capability
  validation.
- Keep these scripts POSIX-friendly Bash; introducing other interpreters or
  language runtimes complicates distribution.
