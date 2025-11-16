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

* POSIX shell utilities + `bash 3.2`
* `jq`
* `make`
* The `codex` CLI (only if you plan to exercise Codex modes)

The goal is to limit probe noise by keeping things lightweight and compatible with the toolchain shipped in macOS. We use `jq`, which must be installed, but that's merely a concession to sanity and could be relaxed in the future. 

## Usage

Each probe is designed to produce a “[boundary object](https://en.wikipedia.org/wiki/Boundary_object)”, in this case a structured JSON output designed to be easy to aggregate and sift through should you generate a few thousand different kinds. Expectations and options are detailed in [docs/boundary-object.md](docs/boundary-object.md).

Run a single probe in a chosen mode:

```sh
bin/fence-run baseline fs_outside_workspace
```

Matrix all probes across all modes and store the JSON output in `out/`:

```sh
make matrix
```

## Tests

Run the fast authoring checks with:

```sh
make test
```

The test runner (`tests/run.sh`) executes four lightweight suites:

* `static_probe_contract` – lints every probe for the documented Bash contract (shebang, `set -euo pipefail`, syntax, ID wiring).
* `capability_map_sync` – keeps `spec/capabilities.yaml`, `spec/capabilities-coverage.json`, and the probes in sync.
* `boundary_object_schema` – validates the `bin/emit-record` output against the cfbo-v1 structure using `jq` only.
* `harness_smoke` – runs a fixture probe through `bin/fence-run` baseline mode to prove the orchestration pipeline still works.

## How probes work

At a high level, a probe is a tiny, single-purpose program plus a contract for how it reports what happened.

### What a probe does

Each probe:

* Exercises **one concrete behavior**: e.g. “read outside the workspace”, “write into `~/.ssh`”, “open a network socket”, “run `sysctl kern.boottime`”, etc.
* Treats the environment as a black box. It doesn’t try to introspect Codex or the host; it just performs its action and looks at:

  * Was it allowed or denied?
  * What error codes, signals, or messages did it see?
  * Did it appear to partially succeed?

The same probe is run multiple times under different modes (baseline shell, Codex sandbox, Codex full-access) so you can compare how the fence shape changes without changing the probe itself.

### What a probe emits

Every probe run must emit exactly one JSON “boundary object” to stdout and then exit. The details are in `docs/boundary-object.md`, but conceptually a boundary object contains:

* **Identity**

  * The probe name (e.g. `"fs_outside_workspace"`).
  * The run mode (e.g. `"baseline"`, `"codex_auto"`, `"codex_full"`).

* **Classification**

  * A small, stable label for the outcome (e.g. `"allowed"`, `"denied"`, `"partial"`, `"error"`).
  * Optionally a more descriptive reason (e.g. `"EACCES"`, `"sandbox_sysctl_read_denied"`, `"host_tool_missing"`).

* **Evidence**

  * Exit status or signal of the attempted operation.
  * Selected stderr/stdout snippets or flags (not full logs) to help distinguish policy changes from normal failures.

The harness treats malformed JSON or missing output as a **probe failure**, not as a sandbox signal. In other words, “my script crashed” is separate from “the fence blocked my operation.”

### What a probe is bound by

When you run:

```sh
bin/fence-run baseline fs_outside_workspace
```

or

```sh
bin/fence-run codex_auto fs_outside_workspace
```

the harness:

1. **Sets up the environment**

   * Chooses a working directory that represents the “workspace” Codex should see.
   * Sets mode-specific environment variables (e.g. which mode you’re in, where to write transient files).
   * Ensures the probe only sees what that mode is supposed to see (e.g. on macOS, Codex modes inherit Seatbelt policies; on Linux, Landlock/seccomp).

2. **Runs the probe under the selected fence**

   * `baseline` runs the probe as a normal shell command.
   * Codex modes invoke the same probe via the `codex` CLI with the appropriate sandbox policy (workspace-only, full-access, etc.).
   * The probe itself doesn’t need to know how sandboxing is implemented; it just runs and measures what happens.

3. **Collects the boundary object**

   * The probe prints a single JSON object to stdout and exits (ideally with status 0).
   * The harness captures that JSON into `out/<mode>/<probe>.json` (or similar) so you can diff across modes, CI runs, or Codex versions.

### Design constraints on probes

To keep the results meaningful and comparable:

* Probes are **non-interactive** (no prompts, no TTY assumptions).
* Probes are **small and focused**: one behavior per probe, no global test orchestration.
* Probes avoid mutating the host outside the workspace unless that mutation is exactly what they’re testing—and if so, they record the target path in the boundary object.
* When a sandbox denies an operation, the probe should still exit cleanly and classify the outcome rather than crashing.

Everything else—the number of modes, how often you run them, and how you analyze the JSON—is left to the harness and whatever tooling you build on top of `codex-fence`.
