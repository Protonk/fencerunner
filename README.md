# codex-fence

`codex-fence` is a lightweight harness for poking at security fences around the [codex](https://github.com/openai/codex) tool. It runs tiny "probe" scripts under several run modes (baseline shell, Codex sandbox, and an experimental Codex full-access mode) and records the results as structured JSON "boundary objects". The tool never talks to models—it simply observes how the runtime fence reacts to filesystem, network, process, or other system interactions.

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

## Operating philosophy

Rather than doing things the clever or efficient way, we're trying something else.

We compile a machine readable catalog of security policy capabilities that we know exist: file system operations, network calls, and the like. Using this catalog we build dozens of tiny probes that hammer at each of these capabilities in different ways. Because we rigidly structure the output of the probes, many different kinds can come together to form a picture of what `codex` can and can't do. What's inside and outside the fence.

This approach has obvious disadvantages. It is clearly inefficient--the repository structure is built around rapid AI generation of probes, some of which are silly or vacuous. In all likelyhood most will never contribute to useful signals about security. It is also **deeply** paranoid, perhaps needlessly so. Certainly past the point of diminishing returns.

However, if we view these disadvantages as choices, benefits appear immediately:
- What Codex can and can't do in your environment is always empirically determined. We don't need to trust `codex` or the os.
- Running many probes is a defense against the security policy surface becoming unexpectedly more complex. Running so many and so many silly ones can (potentially) allow us to capture added complexity that's hard to anticipate.
- With a rigid output structure, disparate probes can be integrated cleanly into signals about capabilities. Weird probes, paranoid probes, even pointless probes that don't add signal cannot contribute to noise. 

## How it works

Everything in the repo exists to turn a capability into an auditable signal:

1. **Catalog** – `schema/capabilities.json` enumerates what we care about (fs,
   network, process, etc.). `docs/capabilities.md` explains the structure while
   `tools/capabilities_adapter.sh` keeps every consumer on the same view.
2. **Probe contract** – A probe under `probes/<probe_id>.sh` binds one
   capability (`primary_capability_id`) to an observable action. The author
   follows [probes/AGENTS.md](probes/AGENTS.md), sources helpers from `lib/`,
   and keeps the script portable Bash.
3. **Execution** – `bin/fence-run <mode> <probe>` exports the run context and
   calls the probe. Helpers in `lib/` give probes predictable utilities, while
   `bin/detect-stack` captures host metadata.
4. **Record emission** – Probes call `bin/emit-record`, which validates CLI
   arguments, pulls capability metadata through the adapter, stamps stack info,
   and serializes a cfbo-v1 document per [docs/boundary_object.md](docs/boundary_object.md).
5. **Signals** – Each run lands in `out/<probe>.<mode>.json`. Comparing those
   boundary objects across modes, commits, or hosts shows how the fence behaves
   in practice.

This pipeline deliberately favors redundancy: machine artifacts (schema,
adapters, cfbo records) are authoritative while the `AGENTS.md` files and docs
explain how to work with them.

### Probe anatomy

- **One probe, one action.** Start with `#!/usr/bin/env bash` +
  `set -euo pipefail`, perform a single operation, and normalize the observed
  result to `success`, `denied`, `partial`, or `error`.
- **Helper reuse.** Reach for `lib/*.sh` (portable path helpers, metadata
  collectors, etc.) before reinventing plumbing. These helpers are pure so they
  are safe to source from probes and tests alike.
- **Emit one record.** Build a concise payload, format the actual command you
  ran, and call `bin/emit-record --run-mode "$FENCE_RUN_MODE" ...` exactly
  once. stdout must only contain the JSON boundary object.
- **Document intent.** Reference the chosen capability ID, describe the
  attempted operation, and keep comments short; downstream agents depend on the
  metadata rather than prose to reason about coverage.

See [docs/probes.md](docs/probes.md) for a complete walkthrough, including how
`bin/fence-run` manages modes and how cfbo fields map to probe inputs.

### DOES it work?!

Yes! Provisonally! The capability catalog is for macOS only, but everything works identically on the `codex-universal` container and hopefully lots of other places.

Once I decide on an API and freeze it I'll retract the "provisionally".

## Repository map

| Path | Role |
| --- | --- |
| `probes/` | Executable probe scripts + author contract; each maps capabilities to observations. |
| `bin/` | Harness entry points (`fence-run`, `emit-record`, `detect-stack`) that bind probes to modes and capture stack data. |
| `lib/` | Pure Bash helpers available to probes/tests (`portable_realpath`, serialization helpers, etc.). |
| `tools/` | Capability adapters/validators that keep metadata consistent across scripts and tests. |
| `schema/` | Machine-readable capability catalog and cfbo schema consumed by bin/tools/tests. |
| `docs/` | Human-readable explanations of catalogs, probes, and boundary objects; use alongside the schema files. |
| `tests/` | Library helpers plus fast-tier and second-tier suites run through `tests/run.sh`; see [tests/AGENTS.md](tests/AGENTS.md). |
| `out/` | Probe boundary objects, one JSON file per `<probe>.<mode>` run, ready for diffing. |
| `Makefile` | Convenience targets (`matrix`, `test`, `probe`, `validate-capabilities`) that glue the harness together. |

Pair this map with [`AGENTS.md`](AGENTS.md) when you need deeper orientation for
any subdirectory.

## Requirements

- POSIX shell utilities + `bash 3.2`
- `jq`
- `python3` (falls back to `python`, then `perl`, for the portable path helpers)
- `make`
- The `codex` CLI (only if you plan to exercise Codex modes)
- Rust toolchain (`cargo`/`rustc`) to build the CLI entrypoints

The goal is to limit probe noise by keeping things lightweight and compatible
with the toolchain shipped in macOS. Stock macOS + the `codex-universal`
container already ship Python (and Perl), so the only additional dependency to
install manually is `jq`.

## Installation

The CLI is a Bash wrapper around Rust helpers that reuse the existing harness.
Install it onto your `PATH` from the repo root:

```sh
make install PREFIX="$HOME/.local"
```

This builds the release binaries and installs `codex-fence`, `fence-bang`,
`fence-listen`, and `fence-test` under `$(PREFIX)/bin`. Keep the cloned repo
around (or set `CODEX_FENCE_ROOT`) so the helpers can find probes, tools, and
tests.

## CLI

Use `codex-fence` for the common workflows:

- `codex-fence --bang` runs the probe matrix and emits cfbo-v1 boundary objects
  as newline-delimited JSON to stdout, following the same mode defaults and
  `PROBES`/`MODES` overrides as `make matrix`.
- `codex-fence --listen` consumes cfbo-v1 JSON from stdin and prints a
  human-readable summary of what succeeded or failed.
- `codex-fence --test` executes the existing `tests/run.sh` harness.

Pipeline example:

```sh
codex-fence --bang | codex-fence --listen
```

## Usage

Each probe run produces a cfbo-v1 boundary object captured under `out/`. Use
these workflows to exercise the harness directly if you need to bypass the CLI:

### Run a single probe

```sh
bin/fence-run baseline fs_outside_workspace
```

Swap `baseline` for `codex-sandbox` or `codex-full` to explore other modes. The
Codex modes require the `codex` CLI on `PATH`.

### Sweep probes across modes

```sh
make matrix
```

The Makefile auto-detects whether the `codex` CLI is available and picks sensible
defaults for `MODES`. Override it to focus on specific modes:

```sh
make matrix MODES="baseline codex-sandbox"
```

The resulting `out/<probe>.<mode>.json` files let you diff policy changes by
mode, Codex version, or host OS.

## Tests and guard rails

Probe development centers on a tight loop plus repo-wide guard rails:

- `tests/run.sh --probe <id>` (or `make probe PROBE=<id>`) lints a single probe
  and enforces the static contract. Use this while editing.
- `make test` runs `tests/run.sh` with no arguments, which first lints every
  probe and then executes the second-tier suites (`capability_map_sync`,
  `boundary_object_schema`, `harness_smoke`, `baseline_no_codex_smoke`, etc.).
- `make validate-capabilities` confirms that probes, fixtures, and stored
  boundary objects only reference cataloged capability IDs.

The guard-rail scripts invoked by those targets live under `tools/`—read
`tools/AGENTS.md` before editing them. When in doubt about a workflow or
directory contract, follow the layered guidance described in the various
`*/AGENTS.md` files.
