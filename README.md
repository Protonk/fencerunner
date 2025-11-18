# codex-fence

`codex-fence` is a lightweight harness for poking at Codex security fences. It runs tiny "probe" scripts under several run modes (baseline shell, Codex sandbox, and an experimental Codex full-access mode) and records the results as structured JSON "boundary objects". The tool never talks to models—it simply observes how the runtime fence reacts to filesystem, network, process, or other system interactions.

## Why?

The "right" way to run an untrusted AI assistant is inside a container where it can't accidentally read your tax returns or delete your home directory.
Nevertheless, I would agree with [Pierce Freeman](https://pierce.dev/notes/a-deep-dive-on-agent-sandboxes) and “wager a large sum that almost no one does that.”

Most developers working with the `codex` CLI will do so on a Mac where the sandboxing policy is officially deprecated and mostly documented by curious outsiders. If you're on Linux things are better but more complicated. What kinds of things can or can't Codex do in your stack? Do you know? How would you know if things changed?

You'd know if you used `codex-fence`. `codex-fence` empirically tests the sandbox boundaries around the `codex` CLI by banging on the fence and seeing what gets through. It is **informed** by documentation but not dependent on system information for its output.

### Ok but... WHY?

Three reasons:

1. The current `codex` security model of three easily distinguished modes may not exist forever. Future modes could have more subtle or difficult-to-interpret surfaces. When that happens, you won't want to be dependent on patch notes to know your new security environment.
2. On macOS, the rough set of rules around sandboxing are a perfect combination of [convenience](https://github.com/openai/codex/issues/215), stability, and opacity. People will get complacent about details.
3. `codex` and "Codex" are very attractive attack vectors. Someone will come for them, or through them. That someone could be anywhere on your stack. Is that paranoid? Yes. [Yes it is](https://en.wikipedia.org/wiki/XZ_Utils_backdoor).

## Requirements

- POSIX shell utilities + `bash 3.2`
- `jq`
- `make`
- The `codex` CLI (only if you plan to exercise Codex modes)

The goal is to limit probe noise by keeping things lightweight and compatible
with the toolchain shipped in macOS. `jq` is the only dependency that is not
part of the default macOS install.

## Probes at a glance

Probes are tiny Bash scripts that perform one observable action and emit a
single JSON boundary object describing what happened. Every probe lives directly
under `probes/<probe_id>.sh` so the filename doubles as the probe id. Probe
authors work from the capability catalog in `spec/capabilities.yaml`, reuse helpers from
`tools/lib/helpers.sh`, and rely on `bin/emit-record` to enforce the cfbo-v1
schema. These scripts intentionally avoid non-portable Bash features so they can run
unchanged on macOS’ `/bin/bash 3.2` and Linux `/usr/bin/env bash`. 

A detailed, human-readable walk-through—including the execution contract,
shared helpers, and how `bin/fence-run` orchestrates modes—lives in
[docs/probes.md](docs/probes.md). The boundary-object schema itself exists in [schema/boundary_object_cfbo_v1.json](schema/boundary_object_cfbo_v1.json) and is described in more detail with examples in [docs/boundary_object.md](docs/boundary_object.md).

## Usage

Each probe run produces a "codex fence boundary object" (`cfbo-v1`) following the above schema that the harness stores under `out/`.

### Run a single probe

```sh
bin/fence-run baseline fs_outside_workspace
```

Use `codex-sandbox` or `codex-full` in place of `baseline` to explore other
fence modes. Codex modes require the `codex` CLI to be installed and available
in `PATH`.

### Run every probe across selected modes

```sh
make matrix
```

The Makefile auto-detects whether the `codex` CLI is available and chooses an
appropriate default for `MODES`. Override it to restrict or expand coverage:

```sh
make matrix MODES="baseline codex-sandbox"
```

Each run lands in `out/<probe>.<mode>.json`, making it easy to diff policy
changes by mode, Codex version, or host OS.

## Tests

Probe development now centers on a fast single-probe loop plus a second tier of
portable validations. The entry point for both is `tests/run.sh`.

- `tests/run.sh --probe <id>` (or `make probe PROBE=<id>`) lints just that
  script and enforces the static probe contract. This is the recommended loop
  while authoring or editing a probe.
- `make test` runs `tests/run.sh` with no arguments, which:
  1. Runs the fast tier (light lint + static probe contract) across every
     probe script under `probes/`.
  2. Executes the second tier suites: `capability_map_sync`,
     `boundary_object_schema`, `harness_smoke`, and `baseline_no_codex_smoke`
     (which hides `codex` from `PATH` to prove baseline stays portable).
- `make validate-capabilities` is available any time you need to confirm that
  probes, fixtures, and stored boundary objects only reference capability ids
  defined in the catalog.
