//! Lightweight parsing of probe scripts for metadata.
//!
//! The helpers here scrape Bash probes for the identifiers the harness needs
//! (probe name, primary/secondary capabilities) without executing the scripts.
//! They intentionally err on the side of under-reporting when values look
//! dynamic because the outputs drive coverage accounting and validation.

use crate::catalog::CapabilityId;
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
/// Partial probe metadata scraped from a shell script.
///
/// Fields remain optional when static parsing cannot determine a value; callers
/// surface missing requirements with file context.
pub struct ProbeMetadata {
    pub script: PathBuf,
    pub probe_name: Option<String>,
    pub probe_version: Option<String>,
    pub primary_capability: Option<CapabilityId>,
    pub secondary_capabilities: Vec<CapabilityId>,
}

impl ProbeMetadata {
    /// Extract metadata assignments from a probe script.
    ///
    /// Parsing is heuristic: the function looks for simple variable assignments
    /// and ignores commented lines rather than attempting a full shell parse.
    pub fn from_script(path: &Path) -> Result<Self> {
        let contents =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Ok(Self {
            script: fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
            probe_name: parse_assignment(&contents, "probe_name"),
            probe_version: parse_assignment(&contents, "probe_version"),
            primary_capability: parse_assignment(&contents, "primary_capability_id")
                .map(CapabilityId),
            secondary_capabilities: parse_secondary_capabilities(&contents),
        })
    }
}

/// Collect every `.sh` script under the provided roots.
///
/// Directory traversal is recursive to support fixture locations used by tests.
pub fn collect_probe_scripts(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut scripts = Vec::new();
    for root in paths {
        collect_from_dir(root, &mut scripts)?;
    }
    scripts.sort();
    Ok(scripts)
}

fn collect_from_dir(root: &Path, acc: &mut Vec<PathBuf>) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_from_dir(&path, acc)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("sh") {
            acc.push(path);
        }
    }
    Ok(())
}

fn parse_assignment(contents: &str, var: &str) -> Option<String> {
    let prefix = var;
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix(prefix) else {
            continue;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('=') {
            continue;
        }
        let mut value = rest[1..].trim_start();
        if value.is_empty() {
            continue;
        }
        if value.starts_with('"') {
            value = &value[1..];
            if let Some(end) = value.find('"') {
                return Some(value[..end].to_string());
            }
        } else if value.starts_with('\'') {
            value = &value[1..];
            if let Some(end) = value.find('\'') {
                return Some(value[..end].to_string());
            }
        } else {
            let token = value.split_whitespace().next().unwrap_or("").trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}

fn parse_secondary_capabilities(contents: &str) -> Vec<CapabilityId> {
    let mut ids = BTreeSet::new();
    let mut array_open = false;
    for raw_line in contents.lines() {
        let line = raw_line.split('#').next().unwrap_or("");
        let trimmed = line.trim_start();

        if array_open {
            let (segment, closed) = array_segment(trimmed);
            push_tokens(segment, &mut ids);
            if closed {
                array_open = false;
            }
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("secondary_capability_id=") {
            if let Some(id) = parse_token(value.trim()) {
                ids.insert(id);
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("secondary_capability_ids=(") {
            let (segment, closed) = array_segment(rest);
            push_tokens(segment, &mut ids);
            array_open = !closed;
            continue;
        }

        if trimmed.contains("--secondary-capability-id") {
            push_from_flags(trimmed, &mut ids);
        }
    }

    ids.into_iter().collect()
}

fn push_tokens(text: &str, acc: &mut BTreeSet<CapabilityId>) {
    for token in text.split_whitespace() {
        if let Some(id) = parse_token(token) {
            acc.insert(id);
        }
    }
}

fn push_from_flags(text: &str, acc: &mut BTreeSet<CapabilityId>) {
    let mut parts = text.split_whitespace().peekable();
    while let Some(part) = parts.next() {
        if let Some(rest) = part.strip_prefix("--secondary-capability-id=") {
            if let Some(id) = parse_token(rest) {
                acc.insert(id);
            }
            continue;
        }

        if part == "--secondary-capability-id" {
            if let Some(next) = parts.next() {
                if let Some(id) = parse_token(next) {
                    acc.insert(id);
                }
            }
        }
    }
}

fn array_segment(text: &str) -> (&str, bool) {
    if let Some(pos) = text.find(')') {
        (&text[..pos], true)
    } else {
        (text, false)
    }
}

fn parse_token(raw: &str) -> Option<CapabilityId> {
    let trimmed = raw.trim().trim_matches(|c| c == '"' || c == '\'');
    // Ignore empty tokens and anything containing shell substitution to avoid
    // claiming dynamic IDs that can only be resolved at runtime.
    if trimmed.is_empty() || trimmed.contains('$') {
        return None;
    }
    Some(CapabilityId(trimmed.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_secondary_capabilities_collects_arrays_and_flags() {
        let contents = r#"
secondary_capability_id=cap_a
secondary_capability_ids=(cap_b "cap_c")
some_command --secondary-capability-id cap_d --secondary-capability-id=cap_e
        "#;
        let parsed = parse_secondary_capabilities(contents);
        assert_eq!(
            parsed,
            vec![
                CapabilityId("cap_a".to_string()),
                CapabilityId("cap_b".to_string()),
                CapabilityId("cap_c".to_string()),
                CapabilityId("cap_d".to_string()),
                CapabilityId("cap_e".to_string())
            ]
        );
    }

    #[test]
    fn collect_probe_scripts_recurse_into_nested_dirs() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path();
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let root_script = root.join("root.sh");
        let nested_script = nested.join("nested.sh");
        std::fs::write(&root_script, "#!/bin/sh\n").unwrap();
        std::fs::write(&nested_script, "#!/bin/sh\n").unwrap();

        let scripts = collect_probe_scripts(&[root.to_path_buf()]).expect("collect scripts");
        assert_eq!(scripts.len(), 2);
        assert!(scripts.contains(&root_script));
        assert!(scripts.contains(&nested_script));
    }
}
