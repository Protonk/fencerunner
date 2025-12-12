//! Plain-text listener that turns boundary-object NDJSON into a readable summary.
//!
//! This binary intentionally stays text-only so it can sit in pipelines like
//! `probe --matrix | probe --listen`. It leans on the shared
//! boundary reader so it understands the exact boundary schema without rolling
//! bespoke parsers.

use anyhow::{Context, Result, anyhow, bail};
use fencerunner::{
    BoundaryObject, BoundaryReadError, BoundarySchema, find_repo_root, read_boundary_objects,
    resolve_boundary_schema_path,
};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::io::{self, BufRead, BufReader, IsTerminal};
use std::path::{Path, PathBuf};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse()?;
    let repo_root = find_repo_root().ok();
    let schema_path = resolve_listen_schema_path(repo_root.as_deref(), cli.boundary_schema_path)?;
    let boundary_schema = BoundarySchema::load(&schema_path)
        .with_context(|| format!("loading boundary schema from {}", schema_path.display()))?;
    let stdin = io::stdin();
    if stdin.is_terminal() {
        bail!(
            "probe --listen expects boundary-object NDJSON on stdin (e.g. probe --matrix | probe --listen)"
        );
    }

    let reader = BufReader::new(stdin.lock());
    let mut output = String::new();
    render_listen_output(reader, &mut output, &boundary_schema).map_err(|err| match err {
        ListenError::Boundary(inner) => anyhow!(inner),
        ListenError::Validation(message) => anyhow!(message),
        ListenError::Serialize(inner) => anyhow!(inner),
        ListenError::Write(inner) => anyhow!(inner),
    })?;
    print!("{}", output);
    Ok(())
}

/// Read NDJSON from `reader`, summarize, and render into the provided writer.
pub fn render_listen_output<R: BufRead, W: fmt::Write>(
    reader: R,
    writer: &mut W,
    boundary_schema: &BoundarySchema,
) -> Result<(), ListenError> {
    let records = read_boundary_objects(reader).map_err(ListenError::Boundary)?;
    for record in &records {
        let value = serde_json::to_value(record).map_err(ListenError::Serialize)?;
        boundary_schema
            .validate(&value)
            .map_err(|err| ListenError::Validation(err.to_string()))?;
    }
    render_records(&records, writer).map_err(ListenError::Write)
}

#[derive(Debug, Default)]
struct ListenStats {
    total_records: usize,
    distinct_probes: usize,
    results: BTreeMap<String, usize>,
    modes: BTreeMap<String, usize>,
}

fn summarize_records(records: &[BoundaryObject]) -> ListenStats {
    let mut stats = ListenStats::default();
    stats.total_records = records.len();
    stats.distinct_probes = records
        .iter()
        .map(|record| record.probe.id.as_str())
        .collect::<BTreeSet<_>>()
        .len();

    for record in records {
        *stats
            .results
            .entry(record.result.observed_result.clone())
            .or_insert(0) += 1;
        *stats.modes.entry(record.run.mode.clone()).or_insert(0) += 1;
    }

    stats
}

fn render_records(records: &[BoundaryObject], writer: &mut impl fmt::Write) -> fmt::Result {
    let stats = summarize_records(records);
    render_summary(&stats, writer)?;
    writeln!(writer)?;
    for (idx, record) in records.iter().enumerate() {
        render_record(idx + 1, record, writer)?;
    }
    Ok(())
}

fn render_summary(stats: &ListenStats, writer: &mut impl fmt::Write) -> fmt::Result {
    writeln!(writer, "probe listen summary")?;
    writeln!(writer, "==========================")?;
    writeln!(writer, "total records  : {}", stats.total_records)?;
    writeln!(writer, "distinct probes: {}", stats.distinct_probes)?;
    writeln!(
        writer,
        "results        : {}",
        format_counts(&stats.results, "none")
    )?;
    writeln!(
        writer,
        "modes          : {}",
        format_counts(&stats.modes, "none")
    )?;
    Ok(())
}

fn render_record(idx: usize, record: &BoundaryObject, writer: &mut impl fmt::Write) -> fmt::Result {
    writeln!(
        writer,
        "[#{}] {:<7} mode={} probe={}",
        idx, record.result.observed_result, record.run.mode, record.probe.id
    )?;
    let capability = &record.capability_context.primary;
    writeln!(
        writer,
        "  capability: {} ({}, {})",
        capability.id.0,
        capability.category.as_str(),
        capability.layer.as_str()
    )?;
    writeln!(
        writer,
        "  op:        {} {}",
        record.operation.verb, record.operation.target
    )?;
    if let Some(message) = record
        .result
        .message
        .as_deref()
        .map(str::trim)
        .filter(|msg| !msg.is_empty())
    {
        writeln!(writer, "  message:   {}", message)?;
    }

    write_snippet(writer, "stdout", record.payload.stdout_snippet.as_deref())?;
    write_snippet(writer, "stderr", record.payload.stderr_snippet.as_deref())?;
    writeln!(writer)?;
    Ok(())
}

fn write_snippet(writer: &mut impl fmt::Write, label: &str, snippet: Option<&str>) -> fmt::Result {
    let Some(snippet) = snippet else {
        return Ok(());
    };
    let trimmed = snippet.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    writeln!(writer, "  {}:", label)?;
    let mut lines = trimmed.lines();
    for _ in 0..MAX_SNIPPET_LINES {
        match lines.next() {
            Some(line) => writeln!(writer, "    {}", truncate_line(line))?,
            None => return Ok(()),
        }
    }

    if lines.next().is_some() {
        writeln!(writer, "    …")?;
    }
    Ok(())
}

fn truncate_line(line: &str) -> String {
    let clean = line.trim_end();
    if clean.chars().count() <= MAX_SNIPPET_CHARS {
        return clean.to_string();
    }
    let mut shortened = String::with_capacity(MAX_SNIPPET_CHARS + 1);
    for (idx, ch) in clean.chars().enumerate() {
        if idx >= MAX_SNIPPET_CHARS - 1 {
            shortened.push('…');
            break;
        }
        shortened.push(ch);
    }
    shortened
}

fn format_counts(map: &BTreeMap<String, usize>, empty_label: &str) -> String {
    if map.is_empty() {
        return empty_label.to_string();
    }
    map.iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug)]
pub enum ListenError {
    Boundary(BoundaryReadError),
    Validation(String),
    Serialize(serde_json::Error),
    Write(fmt::Error),
}

struct Cli {
    boundary_schema_path: Option<PathBuf>,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args_os();
        let _program = args.next();
        let mut boundary_schema_path = None;

        while let Some(arg) = args.next() {
            let arg_str = arg
                .to_str()
                .ok_or_else(|| anyhow!("invalid UTF-8 in argument"))?;
            match arg_str {
                "--boundary" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--boundary requires a value"))?;
                    boundary_schema_path = Some(PathBuf::from(
                        value
                            .into_string()
                            .map_err(|_| anyhow!("--boundary must be valid UTF-8"))?,
                    ));
                }
                "--help" | "-h" => usage(0),
                other => bail!("unknown argument: {other}"),
            }
        }

        Ok(Self {
            boundary_schema_path,
        })
    }
}

fn resolve_listen_schema_path(
    repo_root: Option<&Path>,
    override_path: Option<PathBuf>,
) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(repo_relative(repo_root, &path));
    }
    if let Ok(env_path) = env::var("BOUNDARY_PATH") {
        return Ok(repo_relative(repo_root, Path::new(&env_path)));
    }
    if let Some(root) = repo_root {
        return resolve_boundary_schema_path(root, None);
    }
    bail!(
        "Unable to resolve boundary schema path. Set --boundary, BOUNDARY_PATH, or run from a probe repository."
    )
}

fn repo_relative(base: Option<&Path>, candidate: &Path) -> PathBuf {
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else if let Some(root) = base {
        root.join(candidate)
    } else {
        candidate.to_path_buf()
    }
}

fn usage(code: i32) -> ! {
    eprintln!(
        "Usage: probe --listen [--boundary PATH]\n\nOptions:\n  --boundary PATH           Override boundary-object schema path (or set BOUNDARY_PATH).\n  --help                    Show this help text."
    );
    std::process::exit(code);
}

const MAX_SNIPPET_LINES: usize = 3;
const MAX_SNIPPET_CHARS: usize = 160;

#[cfg(test)]
mod tests {
    use super::*;
    use fencerunner::{
        CapabilityCategory, CapabilityContext, CapabilityId, CapabilityLayer, CapabilitySnapshot,
    };
    use std::io::{BufReader, Cursor};

    fn boundary_schema() -> BoundarySchema {
        let repo_root = fencerunner::find_repo_root().expect("repo root");
        let path = resolve_boundary_schema_path(&repo_root, None).expect("resolve boundary schema");
        BoundarySchema::load(&path).expect("load boundary schema")
    }

    #[test]
    fn renders_summary_and_records_for_golden_snippet() {
        let reader = golden_snippet_reader();
        let mut output = String::new();
        render_listen_output(reader, &mut output, &boundary_schema())
            .expect("render should succeed");

        assert!(output.contains("total records  : 3"));
        assert!(
            output.contains("success"),
            "expected success result mentioned"
        );
        assert!(
            output.contains("partial") || output.contains("denied") || output.contains("error"),
            "expected at least one non-success result mentioned"
        );
        assert!(output.contains("[#1]"), "expected record index header");
    }

    #[test]
    fn renders_empty_summary_for_empty_input() {
        let cursor = Cursor::new(Vec::<u8>::new());
        let reader = BufReader::new(cursor);
        let mut output = String::new();
        render_listen_output(reader, &mut output, &boundary_schema())
            .expect("empty input should succeed");
        assert!(output.contains("total records  : 0"));

        let mut record = minimal_record();
        record.payload.stdout_snippet = Some(String::new());
        let ndjson = serde_json::to_string(&record).unwrap();
        let mut buffer = String::new();
        render_listen_output(
            BufReader::new(Cursor::new(ndjson.into_bytes())),
            &mut buffer,
            &boundary_schema(),
        )
        .unwrap();
        assert!(buffer.contains("[#1]"));
        assert!(buffer.contains(&record.probe.id));
    }

    fn golden_snippet_reader() -> BufReader<Cursor<Vec<u8>>> {
        let records = vec![
            minimal_record_with_result("success"),
            minimal_record_with_result("denied"),
            minimal_record_with_result("partial"),
        ];
        let ndjson = records.join("\n");
        BufReader::new(Cursor::new(ndjson.into_bytes()))
    }

    fn minimal_record() -> BoundaryObject {
        let schema = boundary_schema();
        BoundaryObject {
            schema_version: schema.schema_version().to_string(),
            schema_key: schema.schema_key().map(str::to_string),
            capabilities_schema_version: Some(default_catalog_key()),
            stack: fencerunner::StackInfo {
                sandbox_mode: Some("baseline".to_string()),
                os: "Darwin".to_string(),
            },
            probe: fencerunner::ProbeInfo {
                id: "sample_probe".to_string(),
                version: "1".to_string(),
                primary_capability_id: CapabilityId("cap_sample".to_string()),
                secondary_capability_ids: Vec::new(),
            },
            run: fencerunner::RunInfo {
                mode: "baseline".to_string(),
                workspace_root: Some("/tmp".to_string()),
                command: "echo sample".to_string(),
            },
            operation: fencerunner::OperationInfo {
                category: "fs".to_string(),
                verb: "read".to_string(),
                target: "/tmp/sample".to_string(),
                args: serde_json::json!({}),
            },
            result: fencerunner::ResultInfo {
                observed_result: "success".to_string(),
                raw_exit_code: Some(0),
                errno: None,
                message: Some("sample message".to_string()),
                error_detail: None,
            },
            payload: fencerunner::Payload {
                stdout_snippet: Some("hello".to_string()),
                stderr_snippet: None,
                raw: serde_json::json!({}),
            },
            capability_context: CapabilityContext {
                primary: CapabilitySnapshot {
                    id: CapabilityId("cap_sample".to_string()),
                    category: CapabilityCategory::Filesystem,
                    layer: CapabilityLayer::OsSandbox,
                },
                secondary: Vec::new(),
            },
        }
    }

    fn minimal_record_with_result(result: &str) -> String {
        let mut record = minimal_record();
        record.result.observed_result = result.to_string();
        serde_json::to_string(&record).unwrap()
    }

    fn default_catalog_key() -> fencerunner::CatalogKey {
        let repo_root = fencerunner::find_repo_root().expect("repo root");
        let path = fencerunner::default_catalog_path(&repo_root);
        fencerunner::load_catalog_from_path(&path)
            .expect("load catalog")
            .catalog
            .key
            .clone()
    }
}
