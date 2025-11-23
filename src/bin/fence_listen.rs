//! Plain-text listener that turns cfbo-v1 NDJSON into a readable summary.
//!
//! This binary intentionally stays text-only so it can sit in pipelines like
//! `codex-fence --bang | codex-fence --listen`. It leans on the shared
//! boundary reader so it understands the exact cfbo-v1 schema without rolling
//! bespoke parsers.

use anyhow::{Result, anyhow, bail};
use codex_fence::{BoundaryObject, BoundaryReadError, read_boundary_objects};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, BufRead, BufReader, IsTerminal};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        bail!(
            "codex-fence --listen expects cfbo-v1 NDJSON on stdin (e.g. codex-fence --bang | codex-fence --listen)"
        );
    }

    let reader = BufReader::new(stdin.lock());
    let mut output = String::new();
    listen_to_reader(reader, &mut output).map_err(|err| match err {
        ListenError::Boundary(inner) => anyhow!(inner),
        ListenError::Write(inner) => anyhow!(inner),
    })?;
    print!("{}", output);
    Ok(())
}

fn listen_to_reader<R: BufRead, W: fmt::Write>(
    reader: R,
    writer: &mut W,
) -> Result<(), ListenError> {
    let records = read_boundary_objects(reader).map_err(ListenError::Boundary)?;
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
    writeln!(writer, "codex-fence listen summary")?;
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
enum ListenError {
    Boundary(BoundaryReadError),
    Write(fmt::Error),
}

const MAX_SNIPPET_LINES: usize = 3;
const MAX_SNIPPET_CHARS: usize = 160;

#[cfg(test)]
mod tests {
    use super::*;
    use codex_fence::{
        CapabilityCategory, CapabilityContext, CapabilityId, CapabilityLayer, CapabilitySnapshot,
    };
    use std::collections::BTreeSet;
    use std::io::Cursor;

    const OUTPUT_EX: &str = include_str!("../../output-ex.json");

    #[test]
    fn renders_fixture_summary_and_record_metadata() {
        let records = read_boundary_objects(Cursor::new(OUTPUT_EX.as_bytes())).unwrap();
        let mut output = String::new();
        listen_to_reader(Cursor::new(OUTPUT_EX.as_bytes()), &mut output).unwrap();

        assert!(output.contains(&format!("total records  : {}", records.len())));
        assert!(output.contains(&format!(
                "distinct probes: {}",
                records
                    .iter()
                    .map(|r| r.probe.id.as_str())
                    .collect::<BTreeSet<_>>()
                    .len()
            )));

        let first = records.first().unwrap();
        assert!(output.contains("[#1]"));
        assert!(output.contains(&format!("probe={}", first.probe.id)));
        assert!(output.contains(&first.result.observed_result));
        assert!(output.contains(&first.capability_context.primary.id.0));
    }

    #[test]
    fn handles_empty_input_and_blank_stdout() {
        let mut output = String::new();
        listen_to_reader(Cursor::new(Vec::<u8>::new()), &mut output).unwrap();
        assert!(output.contains("total records  : 0"));

        let mut record = minimal_record();
        record.payload.stdout_snippet = Some(String::new());
        let ndjson = serde_json::to_string(&record).unwrap();
        let mut buffer = String::new();
        listen_to_reader(Cursor::new(ndjson.into_bytes()), &mut buffer).unwrap();
        assert!(buffer.contains("[#1]"));
        assert!(buffer.contains(&record.probe.id));
    }

    fn minimal_record() -> BoundaryObject {
        BoundaryObject {
            schema_version: "cfbo-v1".to_string(),
            capabilities_schema_version: Some(codex_fence::CatalogKey(
                "macOS_codex_v1".to_string(),
            )),
            stack: codex_fence::StackInfo {
                codex_cli_version: Some("codex-cli test".to_string()),
                codex_profile: None,
                sandbox_mode: Some("baseline".to_string()),
                os: "Darwin".to_string(),
            },
            probe: codex_fence::ProbeInfo {
                id: "sample_probe".to_string(),
                version: "1".to_string(),
                primary_capability_id: CapabilityId("cap_sample".to_string()),
                secondary_capability_ids: Vec::new(),
            },
            run: codex_fence::RunInfo {
                mode: "baseline".to_string(),
                workspace_root: Some("/tmp".to_string()),
                command: "echo sample".to_string(),
            },
            operation: codex_fence::OperationInfo {
                category: "fs".to_string(),
                verb: "read".to_string(),
                target: "/tmp/sample".to_string(),
                args: serde_json::json!({}),
            },
            result: codex_fence::ResultInfo {
                observed_result: "success".to_string(),
                raw_exit_code: Some(0),
                errno: None,
                message: Some("sample message".to_string()),
                error_detail: None,
            },
            payload: codex_fence::Payload {
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
}
