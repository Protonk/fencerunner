//! Boundary-object types and helpers.
//!
//! Re-exports the boundary types, schema loader, stream parser, and NDJSON
//! reader so callers can import from `boundary::*` without hunting for files.

pub mod lookup;
pub mod read;
pub mod schema;
pub mod stream;
pub mod types;

pub use read::{BoundaryReadError, read_boundary_objects};
pub use schema::BoundarySchema;
pub use stream::parse_json_stream;
pub use types::{
    BoundaryObject, CapabilityContext, ContextInfo, OperationInfo, ProbeContext, ProbeInfo,
    ResultDetails, ResultInfo, RunInfo, StackInfo,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashSet;
    use std::io::{BufReader, Cursor};

    #[test]
    fn parses_golden_snippet_ndjson() {
        let records =
            read_boundary_objects(golden_snippet_reader()).expect("golden snippet parses");
        assert_eq!(records.len(), 3, "golden snippet should have 3 records");

        let has_success = records
            .iter()
            .any(|record| record.result.outcome == "success");
        assert!(has_success, "expected at least one success record");

        let has_non_success = records
            .iter()
            .any(|record| record.result.outcome != "success");
        assert!(
            has_non_success,
            "expected at least one non-success record for variety"
        );

        let unique_probes: HashSet<&str> = records
            .iter()
            .map(|record| record.probe.id.as_str())
            .collect();
        assert!(
            unique_probes.len() > 1,
            "expected multiple distinct probe ids"
        );
    }

    #[test]
    fn ignores_blank_lines() {
        let first = sample_record("probe_one", "success");
        let second = sample_record("probe_two", "partial");
        let ndjson = format!("{first}\n  \n{second}\n");
        let cursor = Cursor::new(ndjson.into_bytes());
        let records = read_boundary_objects(BufReader::new(cursor)).expect("parses with blanks");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].probe.id, "probe_one");
        assert_eq!(records[1].probe.id, "probe_two");
    }

    #[test]
    fn reports_line_numbers_on_parse_error() {
        let first = sample_record("probe_one", "success");
        let ndjson = format!("{first}\n{first}\n{{ invalid json }}\n");
        let cursor = Cursor::new(ndjson.into_bytes());
        let err = read_boundary_objects(BufReader::new(cursor)).expect_err("should fail");
        match err {
            BoundaryReadError::Parse { line, .. } => assert_eq!(line, 3),
            other => panic!("expected parse error, got {:?}", other),
        }
    }

    fn sample_record(probe_id: &str, outcome: &str) -> String {
        json!({
            "probe": {
                "id": probe_id
            },
            "operation": {
                "kind": "fs.read",
                "target": "sample",
                "args": {}
            },
            "result": {
                "outcome": outcome,
                "details": {
                    "exit_code": 0
                }
            },
            "context": {
                "run": {
                    "workspace_root": "/tmp/sample",
                    "command": "/bin/true"
                },
                "stack": {
                    "os": "Darwin 23.4.0 arm64"
                }
            },
            "payload": {
                "stdout_snippet": null,
                "stderr_snippet": null,
                "raw": {}
            }
        })
        .to_string()
    }

    fn golden_snippet_reader() -> BufReader<Cursor<Vec<u8>>> {
        let records = vec![
            sample_record("probe_success", "success"),
            sample_record("probe_denied", "denied"),
            sample_record("probe_partial", "partial"),
        ];
        let ndjson = records.join("\n");
        BufReader::new(Cursor::new(ndjson.into_bytes()))
    }
}
