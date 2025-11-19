use anyhow::{Context, Result};
use codex_fence::find_repo_root;
use std::process::{Command, Stdio};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let repo_root = find_repo_root()?;
    let script = repo_root.join("tests/run.sh");
    let status = Command::new(&script)
        .current_dir(&repo_root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute {}", script.display()))?;

    match status.code() {
        Some(0) => Ok(()),
        Some(code) => std::process::exit(code),
        None => {
            eprintln!("tests/run.sh terminated by signal");
            std::process::exit(1);
        }
    }
}
