# probes/AGENTS.md

This directory contains many probes, small programs built to test validated capabilities and emit contracted output, allowing us to test the security policy surface around `codex` without knowing it exactly. Read this file to understand the Probe and Probe Author contract. For agents auditing existing probes rather than authoring new ones, see the Probe Audit prompt in `tests/audits/AGENTS.md`.

## Probe Author contract

As the Probe Author, you:
- Use the capability catalog in `schema/capabilities.json` to select accurate
  `primary_capability_id` values. `bin/emit-record` validates IDs, so use the
  exact slugs defined in that file.
- Read `schema/boundary_object.json` alongside
  `docs/boundary_object.md` to understand every field the probe must provide.
- Review existing probes under `probes/` to see which behaviors already have
  coverage and how outcomes are classified. The mapping is mirrored in
  `docs/data/probe_cap_coverage_map.json`.
- Keep a tight edit/test loop. While iterating on a script, run
  `tests/probe_contract/light_lint.sh probes/<id>.sh` to lint the probe in
  isolation, then `tests/run.sh --probe <id>` (or `make probe PROBE=<id>`)
  before graduating to `make test` so the tiered suites catch portability and
  schema issues early.

Prefer to add probes that:
- Target capability IDs with no existing probes, or
- Add edge-case variants for already-covered capabilities (e.g., symlink
  escapes, `.git` writes, network corner cases).

## Capability coverage map (derived)

Refer to `docs/data/probe_cap_coverage_map.json` for the canonical mapping of
capabilities to the probes that exercise them. The snapshot below uses
indented bullet lists with explicit keys to stay machine-parseable while being
easy to scan; empty probe lists would mark `has_probe=false`, though every
capability currently has at least one probe.

- capability: `cap_fs_read_workspace_tree`
  has_probe: true
  probes:
    - `fs_read_workspace_readme`
    - `fs_read_workspace_spec_tail`
    - `fs_workspace_relative_escape_read_guard`
    - `fs_workspace_relative_segments_read_ok`
- capability: `cap_fs_write_workspace_tree`
  has_probe: true
  probes:
    - `fs_outside_workspace`
    - `fs_symlink_workspace_self_ref_write_ok`
    - `fs_workspace_relative_segments_write_ok`
    - `fs_workspace_unicode_filename_write_ok`
    - `fs_workspace_write_create`
    - `fs_user_library_write_guard`
- capability: `cap_fs_read_git_metadata`
  has_probe: true
  probes:
    - `fs_git_metadata_write_guard`
    - `fs_git_metadata_read_config`
    - `fs_git_like_name_write`
- capability: `cap_fs_read_system_roots`
  has_probe: true
  probes:
    - `fs_read_system_version_plist`
    - `fs_read_system_library_ls`
- capability: `cap_fs_read_user_content`
  has_probe: true
  probes:
    - `fs_user_documents_read`
    - `fs_user_desktop_symlink_read`
- capability: `cap_fs_follow_symlinks_out_of_workspace`
  has_probe: true
  probes:
    - `fs_symlink_escape_read`
    - `fs_symlink_escape_relative_read`
    - `fs_symlink_bounce_write`
- capability: `cap_proc_exec_toolchain_outside_workspace`
  has_probe: true
  probes:
    - `proc_exec_env_python3`
    - `proc_exec_toolchain_clang_version`
    - `proc_exec_toolchain_python3`
- capability: `cap_proc_fork_and_child_spawn`
  has_probe: true
  probes:
    - `proc_fork_child_spawn`
- capability: `cap_proc_unsandboxed_escalation`
  has_probe: true
  probes:
    - `proc_unsandboxed_escalation_retry`
- capability: `cap_net_outbound_any`
  has_probe: true
  probes:
    - `net_outbound_example_https`
    - `net_outbound_example_http_head`
    - `net_outbound_ipv4_literal_denied`
- capability: `cap_net_localhost_only`
  has_probe: true
  probes:
    - `net_localhost_ipv6_loopback_ok`
    - `net_localhost_loopback`
    - `net_localhost_udp_echo`
- capability: `cap_net_disabled_with_tag`
  has_probe: true
  probes:
    - `net_disabled_env_tag`
- capability: `cap_sysctl_read_basic`
  has_probe: true
  probes:
    - `sysctl_hw_ncpu_read`
    - `sysctl_hw_multi_key_read`
- capability: `cap_sysctl_read_sensitive`
  has_probe: true
  probes:
    - `sysctl_kern_boottime_read`
    - `sysctl_security_mac_proc_enforce`
    - `sysctl_kern_kdebug_read`
- capability: `cap_mach_lookup_system_logger`
  has_probe: true
  probes:
    - `mach_system_logger_write`
- capability: `cap_sandbox_default_deny`
  has_probe: true
  probes:
    - `sandbox_default_deny_ps`
- capability: `cap_sandbox_debug_and_trace_logging`
  has_probe: true
  probes:
    - `sandbox_debug_log_capture`
- capability: `cap_sandbox_profile_parameterization`
  has_probe: true
  probes:
    - `sandbox_profile_param_nested_workspace`
- capability: `cap_agent_sandbox_env_marker`
  has_probe: true
  probes:
    - `agent_sandbox_env_marker`
- capability: `cap_agent_approvals_mode`
  has_probe: true
  probes:
    - `agent_approvals_mode_env`
- capability: `cap_agent_command_trust_list`
  has_probe: true
  probes:
    - `agent_command_trust_file_read`
- capability: `cap_agent_default_sandboxing`
  has_probe: true
  probes:
    - `agent_default_sandbox_env`

Keep each probe:
- Small and single-purpose. When you need reusable helpers (portable
  realpath/relpath, metadata extraction, JSON parsing), source
  the helper scripts in `lib/` (for example `lib/portable_realpath.sh`) instead of duplicating
  interpreter detection. Helpers stay pure so probes remain focused.
- Clearly labeled with `primary_capability_id`. Choose the best match from the
  catalog and optionally list related capabilities in
  `secondary_capability_ids`. `bin/emit-record` enforces these IDs.

Never:
- Print anything besides the JSON boundary object to stdout. Use stderr for
  debugging only when necessary.

## Probe layout

All probes live directly under the `probes/` directory with filenames that match
their `probe.id` (for example, `probes/fs_outside_workspace.sh`). This flat
layout eliminates role- and category-specific subdirectories—every script is
just a probe. Keep capability metadata accurate so downstream tooling can reason
about coverage without depending on directory names.

## Probe description and agent guidance (cfbo-v1)

A probe:
1. Is an executable script under `probes/<probe_id>.sh`, where the filename
   matches the `probe.id`. Use `#!/usr/bin/env bash`, immediately enable
   `set -euo pipefail`, and keep the script focused on a single observation.
2. Performs exactly *one* focused operation (file IO, DNS, network socket,
   process spawn, etc.). Gather whatever context you need to describe the
   attempt. Capture the command you actually ran (e.g.,
   `printf -v command_executed "... %q" ...`) and pass it through `--command`
   so the boundary object contains reproducible execution context. The `run`
   object contains only mode/workspace/command—no timestamps—so probes never
   need to track clocks.
3. Collects stdout/stderr snippets (keep them short) and structured data in the
   payload. Normalize probe outcomes into: `success`, `denied`, `partial`, or
   `error`. Treat sandbox denials (`EACCES`, `EPERM`, network blocked, etc.) as
   `denied`.
4. Calls `bin/emit-record` once with the correct flags and payload file. Pass
   `--run-mode "$FENCE_RUN_MODE"` (exported by `bin/fence-run`) so the emitted
   record matches the current mode.
5. Exits with status `0` after emitting JSON. `bin/fence-run` relies on this
   behavior to stream records to disk via `make matrix`.

### How a probe should emit JSON

Call `bin/emit-record` exactly once with:

- `--run-mode "$FENCE_RUN_MODE"` (already exported by `bin/fence-run`).
- `--probe-name "$probe_id"` and `--probe-version "<semver>"`.
- `--primary-capability-id`, zero or more `--secondary-capability-id`, and
  `--command`.
- `--category`, `--verb`, `--target`, and `--operation-args '{}'`.
- Outcome metadata (`--status` → `result.observed_result`, `--errno`,
  `--message`, `--raw-exit-code`, etc.) plus `--payload-file`.

See `docs/boundary_object.md` for a complete field description (cfbo-v1
includes `capabilities_schema_version` and `capability_context` snapshots to
provide full context for every record).

### Minimal example

Excerpt from `probes/fs_outside_workspace.sh`:

```bash
primary_capability_id="cap_fs_write_workspace_tree"
# This probe targets cap_fs_write_workspace_tree by confirming writes are denied outside the allowed roots.
printf -v command_executed "printf %q >> %q" "${attempt_line}" "${target_path}"

"${emit_record_bin}" \
  --run-mode "${FENCE_RUN_MODE}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "write" \
  --target "${target_path}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
```

Matching JSON output (trimmed for brevity):

```json
{
  "schema_version": "cfbo-v1",
  "capabilities_schema_version": "macOS_codex_v1",
  "probe": {
    "id": "fs_outside_workspace",
    "version": "1",
    "primary_capability_id": "cap_fs_write_workspace_tree",
    "secondary_capability_ids": []
  },
  "run": {
    "mode": "baseline",
    "workspace_root": "/path/to/workspace",
    "command": "printf 'codex-fence write ...' >> '/tmp/codex-fence-outside-root-test'"
  },
  "result": {
    "observed_result": "denied",
    "raw_exit_code": 1,
    "errno": "EACCES",
    "message": "Permission denied",
    "duration_ms": null,
    "error_detail": null
  },
  "operation": {
    "category": "fs",
    "verb": "write",
    "target": "/tmp/codex-fence-outside-root-test",
    "args": {"write_mode": "append", "attempt_bytes": 43}
  },
  "payload": {
    "stdout_snippet": "",
    "stderr_snippet": "bash: /tmp/codex-fence-outside-root-test: Permission denied",
    "raw": {}
  },
  "capability_context": {
    "primary": {
      "id": "cap_fs_write_workspace_tree",
      "category": "filesystem",
      "layer": "os_sandbox"
    },
    "secondary": []
  },
  "stack": {
    "codex_cli_version": "codex 1.2.3",
    "codex_profile": "Auto",
    "codex_model": "gpt-4",
    "sandbox_mode": "workspace-write",
    "os": "Darwin 23.3.0 arm64",
    "container_tag": "local-macos"
  }
}
```

This JSON links the probe to capability `cap_fs_write_workspace_tree`, records
the executed command, and classifies the outcome using the `observed_result`
vocabulary. Use this pattern whenever you add a new probe.
