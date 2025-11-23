# codex-fence
>Empirically maps codex CLI sandbox capabilities with tiny probes and JSON logs.

An empirical security notebook for the [codex CLI](https://github.com/openai/codex). `codex-fence` hammers on
sandbox boundaries with dozens of tiny probes, watches what gets through, and
writes every observation down as JSON. It never talks to models; it just asks
“what can this runtime do?” and refuses to guess.

## TL;DR

- Build the helper binaries: `make build-bin`
- Run everything: `codex-fence --bang`
- Target a subset: `codex-fence --rattle --cap <capability-id>`
- Read what happened: `codex-fence --bang | codex-fence --listen`

Probes are single Bash scripts under `probes/`, one action each, emitting one
cfbo-v1 boundary object via the Rust `emit-record` serializer. The helpers live
in `bin/`; the schemas and docs live in `schema/` and `docs/`; the guard rails
live in `tests/`.

## Why bother?

macOS Seatbelt is quirky, Linux setups vary, and the `codex` security surface
will change faster than release notes can explain. You don’t want to discover a
policy regression because a model exfiltrated your notes. Running a pile of small,
paranoid probes gives you an empirical read on what the fence actually allows
today—no speculation, no trust in host metadata.

## How it works

Everything in the repo exists to turn a capability into an auditable signal:
1. **Capability catalog**: `schema/capabilities.json` lists the behaviors we
   care about (fs, net, proc, sandbox). Rust code indexes it; docs live in
   [docs/capabilities.md](docs/capabilities.md).
2. **Probes**: `probes/<id>.sh` is portable Bash (`set -euo pipefail`), performs
   one focused action, and calls `bin/emit-record` once. Helpers (`bin/` +
   `lib/`) provide path canonicalization, JSON extraction, etc. See [docs/probes.md](docs/probes.md) for probe and Probe Author information.
3. **Execution**: `bin/fence-run <mode> <probe>` exports `FENCE_*` context and
   enforces the requested mode (`baseline`, `codex-sandbox`, `codex-full`).
4. **Serialization**: `emit-record` validates capability IDs, pulls stack info
   from `detect-stack`, and serializes a cfbo-v1 record. Everything is strict:
   bad flags are fatal, capability IDs must exist in the catalog. [docs/boundary_object.md](docs/boundary_object.md) contains field-by-field detail.
5. **Signals**: `codex-fence --bang` streams cfbo-v1 JSON per probe/mode while
   `codex-fence --rattle` does the same for a selected subset. Capture the
   NDJSON anywhere you like to diff across modes, commits, or hosts.

### Probes

Probes are small tests of a validated capability, written to conform to "one probe, one action." They start with `#!/usr/bin/env bash` + `set -euo pipefail`, never read stdin and leave stdout as the only JSON record. Probe Authors use helpers, not bespoke parsing, to structure informative payloads which declare capabilities accurately (`--primary-capability-id`), record the exact command, and normalize status to `success|denied|partial|error`. Portability (macOS `/bin/bash 3.2` and `codex-universal` container are baselines) minimizes noise. 

### Modes

`codex-fence` runs in three modes to test two known `codex` CLI modes.
- `baseline`: run the probe directly.
- `codex-sandbox`: `codex sandbox macos --full-auto -- <probe>`; workspace-write
  Seatbelt profile, requires the Codex CLI.
- `codex-full`: `codex --dangerously-bypass-approvals-and-sandbox -- <probe>`;
  no Seatbelt, also requires the Codex CLI.

When sandbox application is blocked (`sandbox-exec: Operation not permitted`),
`fence-run` emits a `preflight` record with `observed_result=denied` instead of
failing the whole matrix.

## Requirements

- POSIX shell + `bash 3.2`
- `make`
- Rust toolchain (build/sync helpers, run Rust tests)
- `python3` (used by some probes)
- Codex CLI on PATH (only for Codex modes)

## Install & build

```sh
make build-bin          # compile helpers into bin/
make install PREFIX=~/.local   # optional: install codex-fence globally
```

## Common workflows

- Run a single probe: `bin/fence-run baseline fs_outside_workspace`
- Sweep modes: `codex-fence --bang` (override `MODES="baseline codex-sandbox"` to limit modes)
- Iterate on a capability/probe: `codex-fence --rattle --cap cap_fs_read_workspace_tree`
- Stream + listen: `codex-fence --bang | codex-fence --listen`

## Directory map

| Directory | What lives here |
| --- | --- |
| `probes/` | One-action Bash probes plus `probes/AGENTS.md`, the only code that directly exercises the sandbox. |
| `bin/` | Prebuilt helper binaries (`codex-fence`, `fence-run`, `emit-record`, `portable-path`, etc.) synced from `src/bin/` via `make build-bin`. |
| `src/` | Rust sources for every helper CLI and library, including the implementations that feed the binaries under `bin/`. |
| `schema/` | Machine-readable capability catalog and cfbo schema that define the contract probes must honor. |
| `docs/` | Human-readable explanations (`docs/*.md`, root `AGENTS.md`) that interpret the schema, probes, and runtime expectations. |
| `tests/` | Guard-rail code (Rust + fixtures) that enforces the same contracts under `cargo test` and during CI. |
| `tools/` | Author tooling: shell helpers for contract gates, adapters, and other workflows that support probe development. |

## Attitude

This project chooses paranoia and redundancy over cleverness. The catalog is
machine-readable, the probes are noisy and literal, and the records are strict.
If something changes in your fence, you’ll see it in JSON rather than in a blog
post. That’s the point. 
