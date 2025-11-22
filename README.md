# codex-fence

An empirical security notebook for the `codex` CLI. `codex-fence` hammers on
sandbox boundaries with dozens of tiny probes, watches what gets through, and
writes every observation down as JSON. It never talks to models; it just asks
“what can this runtime do?” and refuses to guess.

## TL;DR

- Build the helper binaries: `make build-bin`
- Run everything: `make matrix` (or `codex-fence --bang`)
- Read what happened: `codex-fence --listen < out/*.json`

Probes are single Bash scripts under `probes/`, one action each, emitting one
cfbo-v1 boundary object via the Rust `emit-record` serializer. The helpers live
in `bin/`; the schemas and docs live in `schema/` and `docs/`; the guard rails
live in `tests/`.

## Why bother?

macOS Seatbelt is quirky, Linux setups vary, and the `codex` security surface
will change faster than release notes can explain. You don’t want to discover a
policy regression because a model deleted your notes. Running a pile of small,
paranoid probes gives you an empirical read on what the fence actually allows
today—no speculation, no trust in host metadata.

## How it works (one page version)

1. **Capability catalog**: `schema/capabilities.json` lists the behaviors we
   care about (fs, net, proc, sandbox). Rust code indexes it; docs live in
   `docs/capabilities.md`.
2. **Probes**: `probes/<id>.sh` is portable Bash (`set -euo pipefail`), performs
   one focused action, and calls `bin/emit-record` once. Helpers (`bin/` +
   `lib/`) provide path canonicalization, JSON extraction, etc.
3. **Execution**: `bin/fence-run <mode> <probe>` exports `FENCE_*` context and
   enforces the requested mode (`baseline`, `codex-sandbox`, `codex-full`).
   Codex modes preflight sandbox write access and will emit a `preflight` denial
   record if the host blocks them from creating temp dirs.
4. **Serialization**: `emit-record` validates capability IDs, pulls stack info
   from `detect-stack`, and serializes a cfbo-v1 record. Everything is strict:
   bad flags are fatal, capability IDs must exist in the catalog.
5. **Signals**: Runs land in `out/<probe>.<mode>.json`. Diff across modes,
   commits, or hosts to see policy changes.

See `docs/boundary_object.md` for field-by-field detail; `docs/probes.md` for a
probe author walkthrough.

## Probe contract (runtime reality)

- One probe, one action. `#!/usr/bin/env bash` + `set -euo pipefail`; never read
  stdin; stdout is only the JSON record.
- Use helpers, not bespoke parsing: payloads/args built with `emit-record`
  flags (`--payload-stdout/-stderr`, `--payload-raw-field[-json|-list|-null]`,
  `--operation-arg[...]`). Parse JSON with `bin/json-extract` if you must.
- Declare capabilities accurately (`--primary-capability-id`), record the exact
  command you ran, and normalize status to `success|denied|partial|error` with
  sensible errno/message. If a required tool is missing, fail explicitly.
- Stay portable: macOS `/bin/bash 3.2` and `codex-universal` container are
  baselines. Use `bin/portable-path` for `realpath/relpath`.

## Modes

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
make install PREFIX=~/.local   # optional: install codex-fence/fence-*
```

## Common workflows

- Run a single probe: `bin/fence-run baseline fs_outside_workspace`
- Sweep modes: `make matrix` (or `make matrix MODES="baseline codex-sandbox"`)
- Stream + listen: `codex-fence --bang | codex-fence --listen`

Outputs land in `out/` for diffing.

## Tests & guard rails

- Fast loop: `tools/validate_contract_gate.sh --probe <id>` (or `make probe
  PROBE=<id>`) for static + dynamic contract checks on one probe.
- Repo-wide: `bin/fence-test` runs the static contract across all probes.
- Rust guard rails: `cargo test --test suite` (schema validation, harness
  smokes, dynamic gate coverage, json-extract semantics).

## Repo map

| Path | Role |
| --- | --- |
| `probes/` | Probe scripts + author contract. |
| `src/bin/` | Rust helpers (`codex-fence`, `fence-run`, `emit-record`, `detect-stack`, `portable-path`, `json-extract`, `fence-bang/listen/test`). |
| `bin/` | Synced release helpers from `make build-bin`. |
| `schema/` | Capability catalog, cfbo schema. |
| `docs/` | Human-readable explainers (capabilities, probes, boundary objects). |
| `tools/` | Authoring helpers (contract gate, adapters, path resolvers). |
| `tests/` | Static contract + Rust guard rails. |
| `out/` | Probe outputs (`<probe>.<mode>.json`). |
| `tmp/` | Scratch space. |
| `Makefile` | Convenience targets tying the harness together. |

## Attitude

This project chooses paranoia and redundancy over cleverness. The catalog is
machine-readable, the probes are noisy and literal, and the records are strict.
If something changes in your fence, you’ll see it in JSON rather than in a blog
post. That’s the point. 
