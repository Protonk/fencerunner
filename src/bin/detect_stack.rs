//! Collects host/sandbox metadata for inclusion in boundary objects.
//!
//! The binary is intentionally dependency-free and lightweight because probes
//! invoke it for every record. It emits a JSON `StackInfo` snapshot.

use anyhow::Result;
use serde::Serialize;
use std::env;
use std::process::Command;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    ensure_no_args();
    // Keep the OS string compact; this output is embedded in every boundary record.
    let os_info = detect_uname(&["-srm"]).unwrap_or_else(|| fallback_os_info());

    let info = StackInfo { os: os_info };

    println!("{}", serde_json::to_string(&info)?);
    Ok(())
}

#[derive(Serialize)]
struct StackInfo {
    os: String,
}

fn ensure_no_args() {
    let mut args = env::args().skip(1);
    if let Some(first) = args.next() {
        // detect-stack is intentionally flag-free to keep probes simple.
        if matches!(first.as_str(), "-h" | "--help") {
            usage_and_exit();
        }
        usage_and_exit();
    }
}

fn detect_uname(args: &[&str]) -> Option<String> {
    let output = Command::new("uname").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

fn fallback_os_info() -> String {
    format!("{} {}", env::consts::OS, env::consts::ARCH)
}

fn usage_and_exit() -> ! {
    eprintln!("Usage: detect-stack");
    std::process::exit(1);
}
