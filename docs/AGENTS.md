# Guidance for Future Agents

`codex-fence` is intentionally tiny so you can add new probes quickly. When you need to expand the suite,
follow the contract below to keep the tooling predictable.

## Adding a probe

1. Create an executable script under `probes/` (e.g. `probes/my_probe.sh`). Use `#!/usr/bin/env bash`
   and enable `set -euo pipefail`.
2. Perform exactly *one* focused operation inside the probe (a single file write, DNS lookup, process spawn,
   etc.). Gather whatever context you need to describe the attempt.
3. Collect stdout/stderr snippets (keep them short) and any structured data you want in the payload.
4. Call `bin/emit-record` once with the correct flags and payload file. Pass `--run-mode "$FENCE_RUN_MODE"`
   so the emitted record matches the mode selected by `bin/fence-run`.
5. Exit with status `0` after emitting JSON. `bin/fence-run` depends on this behavior to stream records to
   disk via `make matrix`.

You should never print anything besides the JSON boundary object to stdout. Use stderr for debugging only
when necessary.

## Run modes

`bin/fence-run` is the canonical place where run-mode semantics live:

- `baseline`: executes probes directly on the host.
- `codex-sandbox`: executes probes through `codex sandbox <platform> --full-auto`.
- `codex-full`: placeholder for the least-restricted Codex profile. At the moment it falls back to a direct
  execution so that probes still run.

If you need different behavior for a new mode, update `bin/fence-run` once instead of duplicating the logic
across probes.

## Emitting records

- Always send required metadata via flags (`--probe-name`, `--probe-version`, etc.).
- Keep `payload` small. Store structured extras in the `raw` object rather than logging entire files.
- Validate any inline JSON before passing it to `--operation-args` (e.g. using `jq -n '...'`).
- Remember that `emit-record` invokes `detect-stack` automatically. You do *not* need to gather stack data
  inside the probe itself.

When in doubt, look at `probes/fs_outside_workspace.sh` for a minimal example.
