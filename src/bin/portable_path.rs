//! Portable replacements for `realpath`/`relpath` without external deps.
//!
//! This helper mirrors the behavior expected by probe scripts on both macOS and
//! Linux, avoiding reliance on platform-specific coreutils implementations.

use anyhow::{Result, bail};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match parse_args()? {
        Command::RealPath(path) => {
            if let Some(resolved) = resolve_realpath(&path) {
                println!("{}", resolved.display());
            } else {
                println!();
            }
        }
        Command::RelativePath { target, base } => {
            let relative = compute_relpath(&target, &base);
            println!("{}", relative.display());
        }
        Command::Help => {
            print_usage();
        }
    }
    Ok(())
}

enum Command {
    RealPath(PathBuf),
    RelativePath { target: PathBuf, base: PathBuf },
    Help,
}

fn parse_args() -> Result<Command> {
    let mut args = env::args_os();
    let _program = args.next();

    let Some(subcommand) = args.next() else {
        bail!(usage());
    };

    match subcommand.to_str() {
        Some("realpath") => {
            let Some(target) = args.next() else {
                bail!("realpath expects exactly one argument");
            };
            if args.next().is_some() {
                bail!("realpath expects exactly one argument");
            }
            Ok(Command::RealPath(PathBuf::from(target)))
        }
        Some("relpath") => {
            let Some(target) = args.next() else {
                bail!("relpath expects a target path and a base path");
            };
            let Some(base) = args.next() else {
                bail!("relpath expects a target path and a base path");
            };
            if args.next().is_some() {
                bail!("relpath expects only a target path and a base path");
            }
            Ok(Command::RelativePath {
                target: PathBuf::from(target),
                base: PathBuf::from(base),
            })
        }
        Some("--help") | Some("-h") => Ok(Command::Help),
        Some(other) => bail!("Unknown subcommand: {other}"),
        None => bail!("Subcommand must be valid Unicode"),
    }
}

fn usage() -> &'static str {
    "Usage: portable-path <realpath|relpath> [args]\n\nCommands:\n  realpath <path>          Resolve <path> to a canonical absolute path.\n  relpath <path> <base>    Emit the relative path from <base> to <path>.\n"
}

fn print_usage() {
    print!("{}", usage());
}

fn resolve_realpath(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

fn absolute_path(path: &Path) -> PathBuf {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    fs::canonicalize(&candidate).unwrap_or(candidate)
}

fn compute_relpath(target: &Path, base: &Path) -> PathBuf {
    let target_abs = absolute_path(target);
    let base_abs = absolute_path(base);

    let target_components: Vec<_> = target_abs.components().collect();
    let base_components: Vec<_> = base_abs.components().collect();

    let mut shared_prefix_len = 0usize;
    while shared_prefix_len < target_components.len()
        && shared_prefix_len < base_components.len()
        && target_components[shared_prefix_len] == base_components[shared_prefix_len]
    {
        shared_prefix_len += 1;
    }

    let mut relative = PathBuf::new();

    for component in base_components.iter().skip(shared_prefix_len) {
        match component {
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
            _ => relative.push(".."),
        }
    }

    for component in target_components.iter().skip(shared_prefix_len) {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Prefix(prefix) => relative.push(prefix.as_os_str()),
            Component::ParentDir => relative.push(".."),
            Component::Normal(seg) => relative.push(seg),
        }
    }

    if relative.as_os_str().is_empty() {
        relative.push(".");
    }

    relative
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relpath_same_dir() {
        let base = Path::new("/tmp/workspace");
        let target = Path::new("/tmp/workspace/probes/run.sh");
        let rel = compute_relpath(target, base);
        assert_eq!(rel, PathBuf::from("probes/run.sh"));
    }

    #[test]
    fn relpath_parent() {
        let base = Path::new("/tmp/workspace/probes");
        let target = Path::new("/tmp/workspace/docs/spec.md");
        let rel = compute_relpath(target, base);
        assert_eq!(rel, PathBuf::from("../docs/spec.md"));
    }

    #[test]
    fn relpath_identical() {
        let base = Path::new("/tmp/workspace");
        let target = Path::new("/tmp/workspace");
        let rel = compute_relpath(target, base);
        assert_eq!(rel, PathBuf::from("."));
    }
}
