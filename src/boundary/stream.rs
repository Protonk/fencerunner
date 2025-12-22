//! Parse boundary objects from a raw JSON or NDJSON string.
//!
//! This is used by helpers that accept either a single JSON object, an array,
//! or newline-delimited JSON records.

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::types::BoundaryObject;

/// Parse a boundary-object stream from stdin, accepting either NDJSON or a JSON array.
///
/// The parser mirrors the listener contract: empty input is an error, single
/// boundary objects or arrays are accepted, and NDJSON streams are parsed
/// line-by-line so partial writes do not break the whole run.
pub fn parse_json_stream(input: &str) -> Result<Vec<BoundaryObject>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("No input provided on stdin");
    }

    // First try to parse the whole input as a JSON value. If that fails we
    // fall back to the NDJSON line-by-line path below.
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return match value {
            Value::Array(items) => items
                .into_iter()
                .map(serde_json::from_value)
                .collect::<Result<Vec<_>, _>>()
                .context("Unable to parse JSON array of boundary objects"),
            Value::Object(_) => serde_json::from_value(value)
                .map(|obj| vec![obj])
                .context("Unable to parse boundary object"),
            _ => bail!("Unsupported JSON input; expected object or array"),
        };
    }

    let mut records = Vec::new();
    for (idx, line) in trimmed.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let obj: BoundaryObject = serde_json::from_str(line)
            .with_context(|| format!("Unable to parse boundary object from line {}", idx + 1))?;
        records.push(obj);
    }

    if records.is_empty() {
        bail!("No boundary objects found in input stream");
    }

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_json_stream_accepts_ndjson_and_array() {
        let record_json = sample_record_json();
        let serialized = serde_json::to_string(&record_json).expect("serialize sample record");

        let ndjson = format!("{0}\n{0}\n", serialized);
        let nd_records = parse_json_stream(&ndjson).expect("ndjson parses");
        assert_eq!(nd_records.len(), 2);
        assert_eq!(nd_records[0].probe.id, "probe_id");

        let array_input = format!("[{0},{0}]", serialized);
        let array_records = parse_json_stream(&array_input).expect("array parses");
        assert_eq!(array_records.len(), 2);
        let command = array_records[1]
            .context
            .as_ref()
            .and_then(|ctx| ctx.run.as_ref())
            .map(|run| run.command.as_str());
        assert_eq!(command, Some("/bin/true"));
    }

    #[test]
    fn parse_json_stream_rejects_non_objects() {
        assert!(parse_json_stream("").is_err(), "empty input should fail");
        assert!(
            parse_json_stream("42").is_err(),
            "non-object json should fail"
        );
    }

    fn sample_record_json() -> serde_json::Value {
        json!({
            "probe": {
                "id": "probe_id"
            },
            "operation": {
                "kind": "fs.read",
                "target": "/tmp",
                "args": {}
            },
            "result": {
                "outcome": "success",
                "details": {
                    "exit_code": 0
                }
            },
            "context": {
                "capabilities_schema_version": "example_catalog_key",
                "stack": {
                    "os": "Darwin"
                },
                "probe": {
                    "primary_capability_id": "cap_fs_read_workspace_tree",
                    "secondary_capability_ids": []
                },
                "run": {
                    "workspace_root": "/tmp",
                    "command": "/bin/true"
                },
                "capability_context": {
                    "primary": {
                        "id": "cap_fs_read_workspace_tree",
                        "category": "filesystem",
                        "layer": "os_sandbox"
                    },
                    "secondary": []
                }
            },
            "payload": {
                "stdout_snippet": null,
                "stderr_snippet": null,
                "raw": {}
            }
        })
    }
}
