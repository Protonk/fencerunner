#![cfg(unix)]

// CLI and harness behavior guard rails for fencerunner and helper binaries.
mod support;
#[path = "support/common.rs"]
mod common;

use anyhow::{Context, Result};
use fencerunner::resolve_helper_binary;
use serde_json::Value;
use std::fs::{self, File};
use std::process::Command;
use support::{helper_binary, make_executable, repo_root, run_command};
use tempfile::TempDir;

use common::{
    FixtureProbe, TempRepo, parse_boundary_object, repo_guard,
};

// Ensures probe-matrix surfaces malformed probe output without blocking the
// remaining probes from running.
#[test]
fn probe_matrix_continues_after_malformed_probe() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let good = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;
    let broken_contents = r#"#!/usr/bin/env bash
set -euo pipefail
echo not-json
exit 0
"#;
    let broken =
        FixtureProbe::install_from_contents(&repo_root, "tests_malformed_probe", broken_contents)?;

    let mut cmd = Command::new(helper_binary(&repo_root, "probe-matrix"));
    cmd.env(
        "PROBES",
        format!("{},{}", broken.probe_id(), good.probe_id()),
    )
    .env("TEST_PREFER_TARGET", "1");
    let output = cmd
        .output()
        .context("failed to execute probe-matrix with malformed probe")?;

    assert!(
        !output.status.success(),
        "probe-matrix should fail when a probe emits invalid JSON"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected only the valid probe output to remain on stdout"
    );
    let (record, _) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, good.probe_id());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(broken.probe_id()),
        "stderr should mention the malformed probe id; stderr was: {stderr}"
    );

    Ok(())
}

// Smoke-tests the fencerunner --probe CLI end-to-end with a single probe.
#[test]
fn fencerunner_probe_runs_single_probe() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let runner = helper_binary(&repo_root, "fencerunner");
    let mut cmd = Command::new(&runner);
    cmd.arg("--probe").arg(fixture.probe_id())
        .env("TEST_PREFER_TARGET", "1");
    let output = run_command(cmd)?;
    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one record for a single probe"
    );
    let (record, _) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, fixture.probe_id());
    assert!(
        record.run.command.contains("fixture-line"),
        "expected fixture command to mention fixture-line"
    );

    Ok(())
}

// Ensures capability selection resolves the bundled catalog and runs every probe in that slice.
#[test]
fn fencerunner_bundle_runs_capability_subset() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let runner = helper_binary(&repo_root, "fencerunner");
    let mut cmd = Command::new(&runner);
    cmd.arg("--bundle")
        .arg("cap_fs_read_workspace_tree")
        .env("TEST_PREFER_TARGET", "1");
    let output = run_command(cmd)?;
    let stdout = String::from_utf8(output.stdout).context("capability stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert!(
        lines.len() >= 1,
        "capability selection should emit at least one boundary object"
    );
    let mut saw_fixture = false;
    for line in lines {
        let (record, _) = parse_boundary_object(line.as_bytes())?;
        if record.probe.id == fixture.probe_id() {
            saw_fixture = true;
        }
        assert!(!record.run.command.is_empty());
    }
    assert!(
        saw_fixture,
        "capability selection should include the installed fixture probe"
    );

    Ok(())
}

// Error handling: unknown probe id should surface a descriptive failure.
#[test]
fn fencerunner_errors_on_unknown_probe() -> Result<()> {
    let repo_root = repo_root();
    let runner = helper_binary(&repo_root, "fencerunner");
    let output = Command::new(&runner)
        .arg("--probe")
        .arg("does_not_exist")
        .env("TEST_PREFER_TARGET", "1")
        .output()
        .context("failed to execute fencerunner unknown probe")?;
    assert!(
        !output.status.success(),
        "fencerunner should fail for unknown probe ids"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Probe not found"),
        "stderr should explain the unknown probe; got: {stderr}"
    );
    Ok(())
}

// Error handling: unknown capability should be rejected before execution.
#[test]
fn fencerunner_errors_on_unknown_bundle() -> Result<()> {
    let repo_root = repo_root();
    let runner = helper_binary(&repo_root, "fencerunner");
    let output = Command::new(&runner)
        .arg("--bundle")
        .arg("cap_does_not_exist")
        .env("TEST_PREFER_TARGET", "1")
        .output()
        .context("failed to execute fencerunner unknown capability")?;
    assert!(
        !output.status.success(),
        "fencerunner should fail for unknown capabilities"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown capability"),
        "stderr should explain the missing capability; got: {stderr}"
    );
    Ok(())
}

#[test]
fn resolve_helper_prefers_release() -> Result<()> {
    let temp = TempRepo::new();
    let release_dir = temp.root.join("target/release");
    fs::create_dir_all(&release_dir)?;
    let helper = release_dir.join("probe-exec");
    fs::write(&helper, "#!/bin/sh\n")?;
    make_executable(&helper)?;
    let resolved = resolve_helper_binary(&temp.root, "probe-exec")?;
    assert_eq!(resolved, helper);
    Ok(())
}

#[test]
fn bin_helpers_match_manifest() -> Result<()> {
    let repo_root = repo_root();
    let manifest_path = repo_root.join("tools/helpers.manifest.json");
    let manifest: Vec<serde_json::Value> = serde_json::from_reader(File::open(&manifest_path)?)?;
    let mut registered: Vec<String> = manifest
        .into_iter()
        .filter_map(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    registered.sort();

    let mut present = Vec::new();
    for entry in fs::read_dir(repo_root.join("bin"))? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".gitkeep" {
            continue;
        }
        let name_str = name.to_string_lossy().to_string();
        if name_str == "probe-contract-gate" {
            continue;
        }
        present.push(name_str);
    }
    present.sort();

    assert_eq!(
        registered, present,
        "bin/ helpers must match tools/helpers.manifest.json"
    );
    Ok(())
}

#[test]
fn resolve_helper_falls_back_to_bin() -> Result<()> {
    let temp = TempRepo::new();
    let bin_dir = temp.root.join("bin");
    fs::create_dir_all(&bin_dir)?;
    let helper = bin_dir.join("emit-record");
    fs::write(&helper, "#!/bin/sh\n")?;
    make_executable(&helper)?;
    let resolved = resolve_helper_binary(&temp.root, "emit-record")?;
    assert_eq!(resolved, helper);
    Ok(())
}

#[test]
fn fencerunner_prefers_repo_helper() -> Result<()> {
    let repo_root = repo_root();
    let temp_repo = TempDir::new().context("failed to allocate temp repo")?;
    let repo = temp_repo.path();
    let bin_dir = repo.join("bin");
    fs::create_dir_all(&bin_dir)?;
    fs::write(bin_dir.join(".gitkeep"), "")?;
    fs::write(repo.join("Makefile"), "all:\n\t@true\n")?;

    let marker = repo.join("helper_invoked");
    let helper_path = bin_dir.join("probe-matrix");
    fs::write(
        &helper_path,
        "#!/bin/sh\n[ -n \"$MARK_FILE\" ] && echo invoked > \"$MARK_FILE\"\n",
    )?;
    make_executable(&helper_path)?;

    let runner = helper_binary(&repo_root, "fencerunner");
    let output = Command::new(runner)
        .arg("--bang")
        .env("FENCE_ROOT", repo)
        .env("PATH", "")
        .env("MARK_FILE", &marker)
        .output()
        .context("failed to run fencerunner stub")?;

    assert!(output.status.success());
    assert!(marker.is_file());
    Ok(())
}

#[test]
fn fencerunner_falls_back_to_path() -> Result<()> {
    let repo_root = repo_root();
    let temp = TempDir::new().context("failed to allocate temp dir")?;
    let helper_dir = temp.path();
    let marker = helper_dir.join("path_helper_invoked");
    let helper_path = helper_dir.join("probe-listen");
    fs::write(
        &helper_path,
        "#!/bin/sh\n[ -n \"$MARK_FILE\" ] && echo listen > \"$MARK_FILE\"\n",
    )?;
    make_executable(&helper_path)?;

    let source = helper_binary(&repo_root, "fencerunner");
    let runner = helper_dir.join("fencerunner");
    fs::copy(&source, &runner)?;
    make_executable(&runner)?;

    let output = Command::new(&runner)
        .arg("--listen")
        .env("PATH", helper_dir)
        .env_remove("FENCE_ROOT")
        .env("MARK_FILE", &marker)
        .current_dir(helper_dir)
        .output()
        .context("failed to run fencerunner path test")?;

    assert!(output.status.success());
    assert!(marker.is_file());
    Ok(())
}

// --listen should reject any extra flags/args and still require stdin.
#[test]
fn fencerunner_listen_rejects_extra_flags() -> Result<()> {
    let repo_root = repo_root();
    let runner = helper_binary(&repo_root, "fencerunner");
    let output = Command::new(&runner)
        .arg("--listen")
        .arg("--catalog")
        .arg("dummy.json")
        .output()
        .context("failed to execute fencerunner --listen with extra flags")?;
    assert!(
        !output.status.success(),
        "--listen should fail when extra flags are provided"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("takes no additional flags"),
        "stderr should mention exclusivity; got: {stderr}"
    );
    Ok(())
}

#[test]
fn fencerunner_exports_root_to_helpers() -> Result<()> {
    let repo_root = repo_root();
    let temp_repo = TempDir::new().context("failed to allocate temp repo")?;
    let repo = temp_repo.path();
    let bin_dir = repo.join("bin");
    fs::create_dir_all(&bin_dir)?;
    fs::write(bin_dir.join(".gitkeep"), "")?;
    fs::write(repo.join("Makefile"), "all:\n\t@true\n")?;

    let marker = repo.join("root_seen");
    let helper_path = bin_dir.join("probe-matrix");
    fs::write(
        &helper_path,
        "#!/bin/sh\n[ -n \"$FENCE_ROOT\" ] && echo \"$FENCE_ROOT\" > \"$MARK_FILE\"\n",
    )?;
    make_executable(&helper_path)?;

    let runner = helper_binary(&repo_root, "fencerunner");
    let output = Command::new(runner)
        .arg("--bang")
        .env("FENCE_ROOT", repo)
        .env("PATH", "")
        .env("MARK_FILE", &marker)
        .output()
        .context("failed to run probe env propagation test")?;

    assert!(output.status.success());
    let recorded = fs::read_to_string(&marker).context("marker missing")?;
    assert_eq!(fs::canonicalize(recorded.trim())?, fs::canonicalize(repo)?);
    Ok(())
}

#[test]
fn detect_stack_reports_expected_sandbox_modes() -> Result<()> {
    let repo_root = repo_root();
    let detect_stack = helper_binary(&repo_root, "detect-stack");

    let stack_output = run_command(Command::new(&detect_stack))?;
    let stack_json: Value = serde_json::from_slice(&stack_output.stdout)?;
    assert!(
        stack_json
            .get("sandbox_mode")
            .map(|v| v.is_null())
            .unwrap_or(true)
    );

    let override_val = "custom-mode";
    let mut override_cmd = Command::new(&detect_stack);
    override_cmd.env("FENCE_SANDBOX_MODE", override_val);
    let full = run_command(override_cmd)?;
    let full_json: Value = serde_json::from_slice(&full.stdout)?;
    assert_eq!(
        full_json
            .get("sandbox_mode")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
        override_val
    );
    Ok(())
}
