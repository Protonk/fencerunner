use anyhow::{Context, Result};
use codex_fence::{BoundaryObject, CapabilitySnapshot, parse_json_stream};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read},
};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("Failed to read stdin")?;

    let records = parse_json_stream(&input)?;
    print_summary(&records);
    Ok(())
}

#[derive(Debug, Default)]
struct CapabilitySummary {
    category: String,
    layer: String,
    modes: BTreeMap<String, String>,
    probes: BTreeSet<String>,
}

fn print_summary(records: &[BoundaryObject]) {
    let modes = collect_modes(records);
    let capabilities = summarize_capabilities(records);
    let nonsuccess = collect_nonsuccess(records);

    println!("codex-fence listen");
    println!("Records : {}", records.len());
    if !modes.is_empty() {
        println!("Modes   : {}", join(&modes));
    }

    if !capabilities.is_empty() {
        println!("\nCapabilities:");
        for (capability_id, summary) in capabilities {
            let mode_view = summary
                .modes
                .iter()
                .map(|(mode, status)| format!("{mode}={status}"))
                .collect::<Vec<_>>();
            println!(
                "- {} ({} / {}): {}",
                capability_id,
                summary.category,
                summary.layer,
                mode_view.join(", ")
            );
        }
    }

    println!("\nNon-success runs:");
    if nonsuccess.is_empty() {
        println!("- none");
    } else {
        for line in nonsuccess {
            println!("- {line}");
        }
    }
}

fn summarize_capabilities(records: &[BoundaryObject]) -> BTreeMap<String, CapabilitySummary> {
    let mut map: BTreeMap<String, CapabilitySummary> = BTreeMap::new();
    for record in records {
        let snapshot = capability_snapshot(record);
        let entry = map
            .entry(snapshot.id.clone())
            .or_insert_with(|| CapabilitySummary {
                category: snapshot.category.clone(),
                layer: snapshot.layer.clone(),
                ..Default::default()
            });
        entry.modes.insert(
            record.run.mode.clone(),
            record.result.observed_result.clone(),
        );
        entry.probes.insert(record.probe.id.clone());
    }
    map
}

fn capability_snapshot(record: &BoundaryObject) -> CapabilitySnapshot {
    if let Some(ctx) = &record.capability_context {
        return ctx.primary.clone();
    }
    CapabilitySnapshot {
        id: record.probe.primary_capability_id.clone(),
        category: "unknown".to_string(),
        layer: "unknown".to_string(),
    }
}

fn collect_modes(records: &[BoundaryObject]) -> Vec<String> {
    let mut modes: BTreeSet<String> = BTreeSet::new();
    for record in records {
        modes.insert(record.run.mode.clone());
    }
    modes.into_iter().collect()
}

fn collect_nonsuccess(records: &[BoundaryObject]) -> Vec<String> {
    let mut messages = Vec::new();
    for record in records {
        if record.result.observed_result == "success" {
            continue;
        }
        let capability = record
            .primary_capability_id()
            .unwrap_or("unknown-capability");
        let mut detail = format!(
            "{} [{}] -> {} (capability {}, target {})",
            record.probe.id,
            record.run.mode,
            record.result.observed_result,
            capability,
            record.operation.target
        );
        if let Some(msg) = &record.result.message {
            if !msg.is_empty() {
                detail.push_str(&format!(" // {}", msg));
            }
        }
        messages.push(detail);
    }
    messages
}

fn join(values: &[String]) -> String {
    values.join(", ")
}
