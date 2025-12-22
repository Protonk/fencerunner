#![cfg(unix)]

// Probe execution and workspace planning guard rails.
mod support;
#[path = "support/common.rs"]
mod common;

use anyhow::{Context, Result, bail};
use fencerunner::harness::workspace::{
    WorkspaceOverride, canonicalize_path, resolve_probe_metadata, workspace_plan_from_override,
    workspace_tmpdir_plan,
};
use fencerunner::probes::discovery::{Probe, resolve_probe};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::Command;
use support::{helper_binary, make_executable, repo_root, run_command};
use tempfile::{NamedTempFile, TempDir};

use common::{FileGuard, FixtureProbe, TempWorkspace, parse_boundary_object, repo_guard};

// Runs the minimal fixture probe through probe-exec to confirm the
// generated record reflects success and payload propagation.
#[test]
fn harness_smoke_probe_fixture() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let mut probe_cmd = Command::new(helper_binary(&repo_root, "probe-exec"));
    probe_cmd
        .env("TEST_PREFER_TARGET", "1")
        .arg(fixture.probe_id());
    let output = run_command(probe_cmd)?;

    let (record, value) = parse_boundary_object(&output.stdout)?;

    assert_eq!(record.probe.id, fixture.probe_id());
    assert_eq!(record.operation.kind, "fs.read");
    assert_eq!(record.result.outcome, "success");
    assert_eq!(
        value.pointer("/payload/raw/probe").and_then(Value::as_str),
        Some("fixture")
    );
    let workspace_root = record
        .context
        .as_ref()
        .and_then(|ctx| ctx.run.as_ref())
        .and_then(|run| run.workspace_root.as_deref());
    assert_eq!(workspace_root, Some(repo_root.to_str().expect("repo root utf-8")));

    Ok(())
}

// Checks that workspace_root falls back to the caller's cwd when the env hint
// is blank, matching legacy agent expectations.
#[test]
fn workspace_root_fallback() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;
    let temp_run_dir = TempDir::new()?;

    let mut fallback_cmd = Command::new(helper_binary(&repo_root, "probe-exec"));
    fallback_cmd
        .current_dir(temp_run_dir.path())
        .env("FENCE_WORKSPACE_ROOT", "")
        .env("TEST_PREFER_TARGET", "1")
        .arg(fixture.probe_id());
    let output = run_command(fallback_cmd)?;
    let (record, _) = parse_boundary_object(&output.stdout)?;
    let expected_workspace = fs::canonicalize(temp_run_dir.path())?;
    let actual_root = record
        .context
        .as_ref()
        .and_then(|ctx| ctx.run.as_ref())
        .and_then(|run| run.workspace_root.as_deref())
        .expect("workspace_root recorded");
    let actual_workspace = fs::canonicalize(Path::new(actual_root))?;
    assert_eq!(actual_workspace, expected_workspace);

    Ok(())
}

// Exercises the guard rails that keep probe execution inside probes/, blocking
// both absolute paths and escaping symlinks.
#[test]
fn probe_resolution_guards() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();

    let mut script = NamedTempFile::new()?;
    writeln!(script, "#!/usr/bin/env bash")?;
    writeln!(script, "echo should_never_run")?;
    writeln!(script, "exit 0")?;
    let temp_path = script.into_temp_path();
    let outside_script = temp_path.to_path_buf();
    let mut perms = fs::metadata(&outside_script)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&outside_script, perms)?;

    let abs_result = Command::new(helper_binary(&repo_root, "probe-exec"))
        .env("TEST_PREFER_TARGET", "1")
        .arg(&outside_script)
        .output()
        .context("failed to execute probe-exec outside script")?;
    assert!(
        !abs_result.status.success(),
        "probe-exec executed script outside probes/ (stdout: {}, stderr: {})",
        String::from_utf8_lossy(&abs_result.stdout),
        String::from_utf8_lossy(&abs_result.stderr)
    );

    let symlink_path = repo_root.join("probes/tests_probe_resolution_symlink.sh");
    if symlink_path.exists() {
        bail!(
            "symlink fixture already exists at {}",
            symlink_path.display()
        );
    }
    symlink(&outside_script, &symlink_path)?;
    let _symlink_guard = FileGuard {
        path: symlink_path.clone(),
    };

    let symlink_result = Command::new(helper_binary(&repo_root, "probe-exec"))
        .env("TEST_PREFER_TARGET", "1")
        .arg("tests_probe_resolution_symlink")
        .output()
        .context("failed to execute probe-exec via symlink")?;
    assert!(
        !symlink_result.status.success(),
        "probe-exec followed a symlink that escapes probes/ (stdout: {}, stderr: {})",
        String::from_utf8_lossy(&symlink_result.stdout),
        String::from_utf8_lossy(&symlink_result.stderr)
    );

    Ok(())
}

#[test]
fn resolve_probe_prefers_probes_dir() -> Result<()> {
    let workspace = TempWorkspace::new();
    let probes = workspace.root.join("probes");
    fs::create_dir_all(&probes)?;
    let script = probes.join("example.sh");
    fs::write(&script, "#!/usr/bin/env bash\nexit 0\n")?;
    make_executable(&script)?;
    let resolved = resolve_probe(&workspace.root, "example")?;
    assert!(resolved.path.ends_with("example.sh"));
    Ok(())
}

#[test]
fn workspace_override_skip_export() {
    let plan = workspace_plan_from_override(WorkspaceOverride::SkipExport);
    assert!(plan.export_value.is_none());
}

#[test]
fn workspace_override_canonicalizes_path() -> Result<()> {
    let workspace = TempWorkspace::new();
    let plan = workspace_plan_from_override(WorkspaceOverride::UsePath(
        workspace.root.join("probes").into_os_string(),
    ));
    assert!(
        plan.export_value
            .unwrap()
            .to_string_lossy()
            .contains("probes")
    );
    Ok(())
}

#[test]
fn workspace_tmpdir_prefers_workspace_tree() -> Result<()> {
    let workspace = TempWorkspace::new();
    let canonical_root = canonicalize_path(&workspace.root);
    let plan = workspace_plan_from_override(WorkspaceOverride::UsePath(
        canonical_root.clone().into_os_string(),
    ));
    let tmpdir_plan = workspace_tmpdir_plan(&plan, &canonical_root);
    let tmpdir = tmpdir_plan.path.expect("tmpdir");
    assert!(tmpdir.starts_with(&canonical_root));
    assert!(tmpdir.ends_with("tmp"));
    assert!(tmpdir.is_dir());
    Ok(())
}

#[test]
fn workspace_tmpdir_uses_override_when_present() -> Result<()> {
    let workspace = TempWorkspace::new();
    let override_root = workspace.root.join("custom_workspace");
    fs::create_dir_all(&override_root)?;
    let plan = workspace_plan_from_override(WorkspaceOverride::UsePath(
        override_root.clone().into_os_string(),
    ));
    let tmpdir_plan = workspace_tmpdir_plan(&plan, &workspace.root);
    let tmpdir = tmpdir_plan.path.expect("tmpdir");
    let override_canonical = canonicalize_path(&override_root);
    assert!(tmpdir.starts_with(&override_canonical));
    Ok(())
}

#[test]
fn workspace_tmpdir_records_error_when_all_candidates_fail() -> Result<()> {
    let workspace = TempWorkspace::new();
    let override_file = workspace.root.join("override_marker");
    fs::write(&override_file, "marker")?;
    let plan =
        workspace_plan_from_override(WorkspaceOverride::UsePath(override_file.into_os_string()));
    let tmpdir_plan = workspace_tmpdir_plan(&plan, &workspace.root);
    assert!(tmpdir_plan.path.is_none());
    let (attempted, message) = tmpdir_plan.last_error.expect("missing error");
    assert!(!message.is_empty());
    assert!(attempted.ends_with("tmp"));
    Ok(())
}

#[test]
fn resolve_probe_metadata_prefers_script_values() -> Result<()> {
    let workspace = TempWorkspace::new();
    let probes = workspace.root.join("probes");
    fs::create_dir_all(&probes)?;
    let script = probes.join("meta.sh");
    fs::write(
        &script,
        r#"#!/usr/bin/env bash
probe_name="custom_probe"
primary_capability_id="cap_fs_read_workspace_tree"
        "#,
    )?;
    make_executable(&script)?;
    let parsed = fencerunner::probes::metadata::ProbeMetadata::from_script(&script)?;
    let probe = Probe {
        id: "meta".to_string(),
        path: fs::canonicalize(&script)?,
    };
    let resolved = resolve_probe_metadata(&probe, parsed)?;
    assert_eq!(resolved.id, "custom_probe");
    assert_eq!(resolved.primary_capability.0, "cap_fs_read_workspace_tree");
    Ok(())
}
