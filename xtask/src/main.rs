use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(subcommand) = args.next() else {
        return dist(args);
    };

    match subcommand.as_str() {
        "dist" => dist(args),
        "-h" | "--help" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown subcommand: {other}\n\n{}", help_text())),
    }
}

fn dist(args: impl Iterator<Item = String>) -> Result<(), String> {
    let config = DistConfig::parse(args)?;
    if config.help {
        print_dist_help();
        return Ok(());
    }

    let workspace_root = workspace_root()?;
    let version = read_package_version(&workspace_root.join("Cargo.toml"))?;

    let host_target = rustc_host_target()?;
    let (build_target, explicit_target) = match config.target {
        Some(target) => (target, true),
        None => (host_target, false),
    };

    let dist_dir = resolve_output_dir(&workspace_root, &config.out_dir);
    fs::create_dir_all(&dist_dir)
        .map_err(|err| format!("failed to create dist dir {}: {err}", dist_dir.display()))?;

    cargo_build_fencerunner(&workspace_root, explicit_target.then(|| build_target.as_str()))?;

    let built_binary = if explicit_target {
        workspace_root
            .join("target")
            .join(&build_target)
            .join("release")
            .join("fencerunner")
    } else {
        workspace_root.join("target").join("release").join("fencerunner")
    };

    if !built_binary.is_file() {
        return Err(format!(
            "expected built binary at {}, but it does not exist",
            built_binary.display()
        ));
    }

    let artifact_name = format!("fencerunner-v{version}-{build_target}");
    let artifact_path = dist_dir.join(&artifact_name);
    fs::copy(&built_binary, &artifact_path).map_err(|err| {
        format!(
            "failed to copy {} to {}: {err}",
            built_binary.display(),
            artifact_path.display()
        )
    })?;

    write_shasum_file(&dist_dir, &artifact_name)?;
    write_tarball(&dist_dir, &artifact_name)?;
    write_shasum_file(&dist_dir, &format!("{artifact_name}.tar.gz"))?;

    eprintln!("wrote:");
    eprintln!("  {}", artifact_path.display());
    eprintln!("  {}", dist_dir.join(format!("{artifact_name}.sha256")).display());
    eprintln!(
        "  {}",
        dist_dir.join(format!("{artifact_name}.tar.gz")).display()
    );
    eprintln!(
        "  {}",
        dist_dir
            .join(format!("{artifact_name}.tar.gz.sha256"))
            .display()
    );

    Ok(())
}

struct DistConfig {
    help: bool,
    target: Option<String>,
    out_dir: PathBuf,
}

impl DistConfig {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut config = Self {
            help: false,
            target: None,
            out_dir: PathBuf::from("dist"),
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => config.help = true,
                "--target" => config.target = Some(next_value(&mut args, "--target")?),
                "--out-dir" => config.out_dir = PathBuf::from(next_value(&mut args, "--out-dir")?),
                other if other.starts_with('-') => {
                    return Err(format!("unknown option: {other}\n\n{}", dist_help_text()));
                }
                other => {
                    return Err(format!("unexpected argument: {other}\n\n{}", dist_help_text()));
                }
            }
        }

        Ok(config)
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask has no parent dir; cannot locate workspace root".to_string())
}

fn resolve_output_dir(workspace_root: &Path, out_dir: &Path) -> PathBuf {
    if out_dir.is_absolute() {
        out_dir.to_path_buf()
    } else {
        workspace_root.join(out_dir)
    }
}

fn read_package_version(manifest_path: &Path) -> Result<String, String> {
    let manifest = fs::read_to_string(manifest_path)
        .map_err(|err| format!("failed to read {}: {err}", manifest_path.display()))?;

    let mut in_package_section = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_package_section = trimmed == "[package]";
            continue;
        }

        if !in_package_section {
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "version" {
            continue;
        }

        let value = value.split('#').next().unwrap_or(value).trim();
        let Some(value) = value.strip_prefix('"') else {
            return Err(format!(
                "expected version to be a string in {}",
                manifest_path.display()
            ));
        };
        let Some(end_quote) = value.find('"') else {
            return Err(format!(
                "unterminated version string in {}",
                manifest_path.display()
            ));
        };
        return Ok(value[..end_quote].to_string());
    }

    Err(format!(
        "failed to find [package].version in {}",
        manifest_path.display()
    ))
}

fn rustc_host_target() -> Result<String, String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|err| format!("failed to run rustc -vV: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "rustc -vV failed with status {}",
            output.status
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| format!("rustc -vV stdout is not utf-8: {err}"))?;
    for line in stdout.lines() {
        let Some(host) = line.strip_prefix("host:") else {
            continue;
        };
        let host = host.trim();
        if host.is_empty() {
            break;
        }
        return Ok(host.to_string());
    }

    Err("rustc -vV did not include a host target".to_string())
}

fn cargo_build_fencerunner(workspace_root: &Path, target: Option<&str>) -> Result<(), String> {
    let mut command = Command::new("cargo");
    command
        .current_dir(workspace_root)
        .arg("build")
        .arg("--release")
        .arg("--frozen")
        .arg("--package")
        .arg("fencerunner")
        .arg("--bin")
        .arg("fencerunner");

    if let Some(target) = target {
        command.arg("--target").arg(target);
    }

    eprintln!(
        "+ {}",
        command_for_display("cargo", command.get_args().map(OsString::from))
    );
    let status = command
        .status()
        .map_err(|err| format!("failed to execute cargo build: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo build failed with status {status}"))
    }
}

fn write_tarball(dist_dir: &Path, artifact_name: &str) -> Result<(), String> {
    let tar_name = format!("{artifact_name}.tar.gz");
    let mut command = Command::new("tar");
    command
        .current_dir(dist_dir)
        .arg("-czf")
        .arg(&tar_name)
        .arg(artifact_name);

    eprintln!(
        "+ {}",
        command_for_display("tar", command.get_args().map(OsString::from))
    );
    let status = command
        .status()
        .map_err(|err| format!("failed to execute tar: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("tar failed with status {status}"))
    }
}

fn write_shasum_file(dist_dir: &Path, filename: &str) -> Result<(), String> {
    let mut command = Command::new("shasum");
    command
        .current_dir(dist_dir)
        .arg("-a")
        .arg("256")
        .arg(filename);

    eprintln!(
        "+ {}",
        command_for_display("shasum", command.get_args().map(OsString::from))
    );
    let output = command
        .output()
        .map_err(|err| format!("failed to execute shasum: {err}"))?;
    if !output.status.success() {
        return Err(format!("shasum failed with status {}", output.status));
    }

    let sha_file = dist_dir.join(format!("{filename}.sha256"));
    fs::write(&sha_file, output.stdout)
        .map_err(|err| format!("failed to write {}: {err}", sha_file.display()))?;
    Ok(())
}

fn command_for_display(program: &str, args: impl Iterator<Item = OsString>) -> String {
    let mut command = program.to_string();
    for arg in args {
        command.push(' ');
        command.push_str(&shell_escape(&arg));
    }
    command
}

fn shell_escape(arg: &OsString) -> String {
    let s = arg.to_string_lossy();
    if s.is_empty() {
        return "''".to_string();
    }

    if !s.chars().any(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '\\' | '$' | '`')) {
        return s.to_string();
    }

    let mut escaped = String::with_capacity(s.len() + 2);
    escaped.push('\'');
    for c in s.chars() {
        if c == '\'' {
            escaped.push_str("'\\''");
        } else {
            escaped.push(c);
        }
    }
    escaped.push('\'');
    escaped
}

fn print_help() {
    eprintln!("{}", help_text());
}

fn print_dist_help() {
    eprintln!("{}", dist_help_text());
}

fn help_text() -> &'static str {
    "xtask helpers.\n\nUsage:\n  cargo dist [dist options]\n\nSubcommands:\n  dist   Build and package release artifacts.\n\nRun `cargo dist --help` for dist options."
}

fn dist_help_text() -> &'static str {
    "Build and package a release artifact named from Cargo.toml.\n\nUsage:\n  cargo dist [--target <triple>] [--out-dir <dir>]\n\nOptions:\n  --target <triple>   Build fencerunner for a specific target triple.\n  --out-dir <dir>     Output directory for release artifacts (default: dist).\n  -h, --help          Show this help text."
}

