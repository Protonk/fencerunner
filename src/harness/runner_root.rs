//! Runner-owned, ephemeral "root" directory materialized at runtime.
//!
//! The probe contract expects probes to source `${FENCERUNNER_ROOT}/lib/library.sh`.
//! When fencerunner is installed, those assets are not available on disk, so
//! fencerunner materializes a temporary tree containing:
//! - `lib/library.sh` (probe library)
//! - `bin/emit-record` (shim to fencerunner internal subcommand)
//! - `bin/commit-help-me` (shim to fencerunner internal subcommand)

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const LIBRARY_SH: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lib/library.sh"));

pub struct RunnerRoot {
    _tempdir: TempDir,
    path: PathBuf,
    bin_dir: PathBuf,
    lib_dir: PathBuf,
}

impl RunnerRoot {
    pub fn create() -> Result<Self> {
        let tempdir = tempfile::Builder::new()
            .prefix("fencerunner-root.")
            .tempdir()
            .context("allocating runner root tempdir")?;
        let path = tempdir.path().to_path_buf();
        let bin_dir = path.join("bin");
        let lib_dir = path.join("lib");
        fs::create_dir_all(&bin_dir)
            .with_context(|| format!("creating runner root {}", bin_dir.display()))?;
        fs::create_dir_all(&lib_dir)
            .with_context(|| format!("creating runner root {}", lib_dir.display()))?;

        write_file(&lib_dir.join("library.sh"), LIBRARY_SH.as_bytes(), false)
            .context("writing runner lib/library.sh")?;

        write_shim(&bin_dir.join("emit-record"), "__emit-record")
            .context("writing shim bin/emit-record")?;
        write_shim(&bin_dir.join("commit-help-me"), "__commit-help-me")
            .context("writing shim bin/commit-help-me")?;

        Ok(Self {
            _tempdir: tempdir,
            path,
            bin_dir,
            lib_dir,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn bin_dir(&self) -> &Path {
        &self.bin_dir
    }

    pub fn lib_dir(&self) -> &Path {
        &self.lib_dir
    }
}

fn write_shim(path: &Path, internal_command: &str) -> Result<()> {
    let body = format!(
        r#"#!/bin/bash
set -euo pipefail

exec "${{FENCERUNNER_BIN:?FENCERUNNER_BIN not set}}" {internal_command} "$@"
"#
    );
    write_file(path, body.as_bytes(), true)?;
    Ok(())
}

fn write_file(path: &Path, bytes: &[u8], executable: bool) -> Result<()> {
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))?;
    if executable {
        make_executable(path)?;
    }
    Ok(())
}

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}
