//! NDJSON stream reader for boundary objects.
//!
//! Reads newline-delimited JSON into typed records and reports line numbers on
//! parse errors for clearer diagnostics.

use serde_json;
use std::fmt;
use std::io::BufRead;

use super::types::BoundaryObject;

/// Errors that can occur while reading NDJSON boundary object streams.
#[derive(Debug)]
pub enum BoundaryReadError {
    Io(std::io::Error),
    Parse {
        line: usize,
        error: serde_json::Error,
    },
}

impl fmt::Display for BoundaryReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoundaryReadError::Io(err) => write!(f, "failed to read NDJSON stream: {err}"),
            BoundaryReadError::Parse { line, error } => {
                write!(f, "line {line}: unable to parse boundary object ({error})")
            }
        }
    }
}

impl std::error::Error for BoundaryReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BoundaryReadError::Io(err) => Some(err),
            BoundaryReadError::Parse { error, .. } => Some(error),
        }
    }
}

/// Read boundary objects from an NDJSON stream.
///
/// Lines containing only whitespace are skipped. Errors include the 1-based
/// line number where parsing failed to simplify diagnostics for callers.
pub fn read_boundary_objects<R: BufRead>(
    reader: R,
) -> Result<Vec<BoundaryObject>, BoundaryReadError> {
    // Streaming read so large NDJSON inputs do not need to fit in memory.
    let mut records = Vec::new();
    let mut line_buf = String::new();
    let mut reader = reader;
    let mut line_number = 0usize;

    loop {
        line_buf.clear();
        let bytes = reader
            .read_line(&mut line_buf)
            .map_err(BoundaryReadError::Io)?;
        if bytes == 0 {
            break;
        }
        line_number += 1;
        let trimmed = line_buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record = serde_json::from_str::<BoundaryObject>(trimmed).map_err(|error| {
            BoundaryReadError::Parse {
                line: line_number,
                error,
            }
        })?;
        records.push(record);
    }

    Ok(records)
}
