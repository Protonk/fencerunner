//! Internal subcommand backing the script library enrollment helper.
//!
//! This records a single `(commitment_id, help)` pair for the current script run.
//! It does *not* validate the pair against `<RUN_DIR>/commitments.json`; the
//! project treats enrollments as a trustworthy signal from a willing author.
//!
//! Exits 0 on success, 1 on any error (invalid args, missing env, duplicates).

use crate::commitments::model::CommitmentHelp;
use anyhow::{Context, Result, anyhow, bail};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const ENV_ENROLLMENTS_PATH: &str = "FENCERUNNER_COMMITMENT_ENROLLMENTS_PATH";

pub fn run(args: &[std::ffi::OsString]) -> Result<()> {
    let args = CliArgs::parse(args)?;
    let enrollments_path = resolve_enrollments_path()?;
    append_enrollment(&enrollments_path, &args.commitment_id, args.help)
        .with_context(|| format!("recording enrollment at {}", enrollments_path.display()))?;
    Ok(())
}

struct CliArgs {
    help: CommitmentHelp,
    commitment_id: String,
}

impl CliArgs {
    fn parse(args: &[std::ffi::OsString]) -> Result<Self> {
        let mut positionals: Vec<String> = Vec::new();

        for arg in args {
            let arg = arg
                .to_str()
                .ok_or_else(|| anyhow!("invalid UTF-8 in argument"))?;
            match arg {
                "-h" | "--help" => usage(0),
                _ if arg.starts_with("--") => bail!("unknown option: {arg}"),
                _ => positionals.push(arg.to_string()),
            }
        }

        if positionals.len() != 2 {
            usage(1);
        }

        let help = parse_help(&positionals[0])?;
        let commitment_id = positionals[1].clone();
        validate_commitment_id(&commitment_id)?;

        Ok(Self {
            help,
            commitment_id,
        })
    }
}

fn usage(code: i32) -> ! {
    eprintln!(
        "Usage: commit-help-me <ensure|detect|emit> <commitment-id>\n\nEnvironment:\n  {ENV_ENROLLMENTS_PATH}  File path where enrollments are recorded as 'id|help' lines."
    );
    std::process::exit(code);
}

fn parse_help(value: &str) -> Result<CommitmentHelp> {
    match value {
        "ensure" => Ok(CommitmentHelp::Ensure),
        "detect" => Ok(CommitmentHelp::Detect),
        "emit" => Ok(CommitmentHelp::Emit),
        other => bail!("unknown help verb '{other}' (expected ensure|detect|emit)"),
    }
}

fn resolve_enrollments_path() -> Result<PathBuf> {
    let raw = env::var_os(ENV_ENROLLMENTS_PATH)
        .ok_or_else(|| anyhow!("{ENV_ENROLLMENTS_PATH} is not set"))?;
    if raw.is_empty() {
        bail!("{ENV_ENROLLMENTS_PATH} is empty");
    }
    Ok(PathBuf::from(raw))
}

fn append_enrollment(path: &Path, commitment_id: &str, help: CommitmentHelp) -> Result<()> {
    let pair = format!("{commitment_id}|{}", help_label(help));

    // Best-effort duplicate detection by scanning the existing file.
    // This stays intentionally simple; scripts are expected to call commit_help_me
    // in a single-process flow.
    if let Ok(existing) = fs::read_to_string(path) {
        for line in existing.lines() {
            if line.trim() == pair {
                bail!("duplicate enrollment: {pair}");
            }
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    writeln!(file, "{pair}")?;
    Ok(())
}

fn help_label(help: CommitmentHelp) -> &'static str {
    match help {
        CommitmentHelp::Ensure => "ensure",
        CommitmentHelp::Detect => "detect",
        CommitmentHelp::Emit => "emit",
    }
}

fn validate_commitment_id(value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("commitment id must not be empty");
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        bail!("commitment id must match ^[A-Za-z0-9_.-]+$");
    }
    Ok(())
}
