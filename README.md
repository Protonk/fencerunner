# codex-fence

> Empirically maps codex CLI sandbox capabilities with tiny probes and JSON logs.

An empirical security notebook for the codex CLI. `codex-fence` hammers on sandbox boundaries with dozens of tiny probes, watches what gets through, and writes every observation down as JSON. It never talks to models; it just asks “what can this runtime do?” and refuses to guess.

## Why bother?

macOS Seatbelt is quirky, Linux setups vary, and the `codex` security surface will change faster than release notes can explain. You don’t want to discover a policy regression because a model exfiltrated your notes. Running a pile of small, paranoid probes gives you an empirical read on what the fence actually allows today—no speculation, no trust in host metadata.

## Many wild probes with low noise

At a high level, this repo is a structured to allow wild probes to explore system behavior without inducing noise. It contains a number of probes already and is designed for generation of more in a tight loop. Three core pieces--a capability catalog, a suite of probes, and a boundary-object schema--are held in sync by tests and contracts enforced in code.

### Capability catalog

The capability catalog in `schema/capabilities.json` (described in [docs/capabilities.md](docs/capabilities.md)) is the project’s ground truth about “things Codex might be allowed to do”: for example, reading user documents, opening outbound TCP sockets, spawning processes, or inspecting sandbox state. It names behaviors that can be mediated (filesystem, network, process, sandbox, etc.) as machine-readable capabilities.

Each capability entry contains:

* A stable id — what we call the demonstrated capability.
* A category — what kind of system interaction it mediates.
* A policy layer — what is doing the mediation (for example, the operating system or `codex`).
* Human-readable description and notes.

Probes and tooling never invent new capabilities on the fly; they pick from this catalog so coverage can be reasoned about and compared.

### Probes and Probe Authors

Probes are small, focused Bash scripts under `probes/<probe_id>.sh`. They are the only code in the repo that directly touches the sandboxed runtime. Everything else is harness, infrastructure, or interpretation. Probes connect catalog to boundary object: they exercise specific capabilities and emit a single JSON record.

The Probe Author contract is defined in detail at [probes/AGENTS.md](probes/AGENTS.md). In short, a probe:

1. Lives at `probes/<probe_id>.sh` and uses `#!/usr/bin/env bash` with `set -euo pipefail`.
2. Performs exactly one focused operation (for example, one write attempt outside the workspace, one DNS query, one `sysctl` read).
3. Calls `bin/emit-record` exactly once with the right flags to produce a cfbo-v1 JSON record, including `--run-mode "$FENCE_RUN_MODE"` exported by `bin/fence-run`.
4. Prints that JSON record to stdout and exits with status 0 so the harness can treat the probe as “observed” even when the underlying operation is denied.

Probe Authors do not need to know Rust internals, sandbox profile syntax, or Codex implementation details. They just need to satisfy their contract and let the surrounding infrastructure keep everything coherent.
When a probe needs low-level or performance-sensitive behavior, it may shell
out to a small compiled helper under `probe-runtime/` (synced into `bin/` by
`make build-bin`). Helpers stay quiet on stdout and return stable exit codes;
the probe remains the orchestrator and still emits the single boundary object.

### Boundary objects (cfbo-v1)

Every probe run must emit exactly one boundary object, a JSON document with schema version `cfbo-v1` defined in `schema/boundary_object.json` and described in `docs/boundary_object.md`. This defines exactly what each observation must record, allowing many probes to map cleanly from capability to record.

A record contains, among other fields:

* `probe`: who ran (id, version).
* `run`: how it ran (mode, command, timestamps, exit code).
* `operation`: what was attempted (category, verb, target, operation args).
* `result`: what actually happened (`observed_result`, errno/message, payload).
* `capability_context`: snapshots of the primary and secondary capabilities the probe claims to exercise.

The goal is that a single record, plus the catalog snapshot it points at, is a portable explanation of “this runtime could or could not do X under these conditions.”

## Quick start

### Requirements

* POSIX shell with `bash` 3.2 or newer
* `make`
* Rust toolchain (to build helper binaries and run tests)
* `python3` (used by some probes)
* Codex CLI on `PATH` (required for Codex modes; baseline mode can run without it)

### Build and (optionally) install

To compile the helper binaries into `bin/`:

```sh
make build-bin
```

To install the main CLI on your `PATH` (adjust `PREFIX` as needed):

```sh
make install PREFIX=~/.local
```

Ensure `~/.local/bin` (or your chosen prefix) is on your `PATH`.

You can also invoke the compiled binary directly from `target/release/codex-fence` if you prefer building with `cargo`.

### Run probes

Run the full matrix of probes in all available modes (defaults to `baseline`, plus `codex-sandbox` and `codex-full` when the Codex CLI is on `PATH`) and stream NDJSON to stdout:

```sh
codex-fence --bang
```

Limit the run to a subset of modes:

```sh
MODES="baseline codex-sandbox codex-full" codex-fence --bang
```

With `--rattle` you can select a capability or probe by id and optionally restrict the modes with `--mode` (repeatable):

```sh
codex-fence --rattle --cap cap_fs_read_workspace_tree --mode codex-sandbox --mode codex-full
codex-fence --rattle --probe fs_outside_workspace --mode baseline
```

### Inspect what happened

`codex-fence --bang` emits one JSON record per probe run (cfbo-v1) as NDJSON.

For a quick human view, pipe into the listener:

```sh
codex-fence --bang | codex-fence --listen
```

## Attitude

This project chooses paranoia and redundancy over cleverness. The catalog is machine-readable, the probes are noisy and literal, and the records are strict. If something changes in your fence, you will see it in JSON rather than in a blog post. That is the point.
