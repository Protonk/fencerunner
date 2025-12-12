//! paging-stress: small helper to place light paging pressure on the host.
//!
//! This binary intentionally stays quiet (stdout is unused) and surfaces
//! progress only through its exit codes:
//! - 0: workload completed
//! - 1: invalid arguments
//! - 2: internal error (allocation or runtime failure)
//! - 3: self-enforced timeout reached
//!
//! CLI:
//! - `--megabytes <N>` — total allocation size in MiB (default: 8).
//! - `--passes <N>` — number of full sweeps to perform (default: 1).
//! - `--pattern <sequential|random>` — page access order (default: sequential).
//! - `--max-seconds <N>` — optional self-enforced timeout.
//! - `--help` — print usage.
//!
//! Probes invoke this helper with explicit arguments and interpret only the
//! exit code so the probe contract (single JSON record, no stdout) stays
//! intact.

use std::io::{self, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

const DEFAULT_MEGABYTES: usize = 8;
const DEFAULT_PASSES: u64 = 1;
const DEADLINE_CHECK_INTERVAL: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pattern {
    Sequential,
    Random,
}

#[derive(Clone, Debug)]
struct Config {
    total_bytes: usize,
    passes: u64,
    pattern: Pattern,
    max_seconds: Option<u64>,
}

enum ParseOutcome {
    Help,
    Run(Config),
}

enum RunError {
    Timeout { elapsed: Duration, limit: Duration },
    Internal(String),
}

fn main() -> ExitCode {
    let program = std::env::args()
        .next()
        .unwrap_or_else(|| "paging-stress".to_string());

    match parse_args(std::env::args().skip(1)) {
        Ok(ParseOutcome::Help) => {
            print_usage(&program, io::stdout());
            ExitCode::SUCCESS
        }
        Ok(ParseOutcome::Run(config)) => match run_workload(&config) {
            Ok(()) => ExitCode::SUCCESS,
            Err(RunError::Timeout { elapsed, limit }) => {
                eprintln!(
                    "paging-stress: reached max-seconds ({:.0?}) after {:.3?}",
                    limit, elapsed
                );
                ExitCode::from(3)
            }
            Err(RunError::Internal(msg)) => {
                eprintln!("paging-stress: internal error: {msg}");
                ExitCode::from(2)
            }
        },
        Err(message) => {
            eprintln!("paging-stress: {message}");
            eprintln!();
            print_usage(&program, io::stderr());
            ExitCode::from(1)
        }
    }
}

fn parse_args<I>(args: I) -> Result<ParseOutcome, String>
where
    I: Iterator<Item = String>,
{
    let mut megabytes = DEFAULT_MEGABYTES;
    let mut passes = DEFAULT_PASSES;
    let mut pattern = Pattern::Sequential;
    let mut max_seconds = None;

    let mut iter = args.peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(ParseOutcome::Help),
            "--megabytes" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| "--megabytes requires a value".to_string())?;
                megabytes = parse_positive_usize(&raw, "--megabytes")?;
            }
            "--passes" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| "--passes requires a value".to_string())?;
                passes = parse_positive_u64(&raw, "--passes")?;
            }
            "--pattern" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| "--pattern requires a value".to_string())?;
                pattern = match raw.as_str() {
                    "sequential" => Pattern::Sequential,
                    "random" => Pattern::Random,
                    _ => {
                        return Err(format!(
                            "unsupported --pattern {raw}; expected sequential or random"
                        ));
                    }
                };
            }
            "--max-seconds" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| "--max-seconds requires a value".to_string())?;
                let value = parse_positive_u64(&raw, "--max-seconds")?;
                if value == 0 {
                    return Err("--max-seconds must be greater than zero".to_string());
                }
                max_seconds = Some(value);
            }
            other if other.starts_with('-') => {
                return Err(format!("unrecognized flag {other}"));
            }
            other => {
                return Err(format!("unexpected positional argument: {other}"));
            }
        }
    }

    if megabytes == 0 {
        return Err("--megabytes must be greater than zero".to_string());
    }
    if passes == 0 {
        return Err("--passes must be greater than zero".to_string());
    }

    let total_bytes = megabytes
        .checked_mul(1024 * 1024)
        .ok_or_else(|| format!("--megabytes {megabytes} is too large to represent"))?;

    Ok(ParseOutcome::Run(Config {
        total_bytes,
        passes,
        pattern,
        max_seconds,
    }))
}

fn run_workload(config: &Config) -> Result<(), RunError> {
    let page_size = page_size();
    if page_size == 0 {
        return Err(RunError::Internal(
            "system page size reported as zero".to_string(),
        ));
    }

    let page_count = page_count(config.total_bytes, page_size);
    let deadline = config.max_seconds.map(Duration::from_secs);

    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(config.total_bytes)
        .map_err(|err| RunError::Internal(format!("failed to allocate memory: {err}")))?;
    buffer.resize(config.total_bytes, 0u8);

    let start = Instant::now();
    match config.pattern {
        Pattern::Sequential => sequential_sweep(
            &mut buffer,
            page_size,
            page_count,
            config.passes,
            start,
            deadline,
        ),
        Pattern::Random => random_sweep(
            &mut buffer,
            page_size,
            page_count,
            config.passes,
            start,
            deadline,
        ),
    }
}

fn sequential_sweep(
    buffer: &mut [u8],
    page_size: usize,
    page_count: usize,
    passes: u64,
    start: Instant,
    deadline: Option<Duration>,
) -> Result<(), RunError> {
    for _ in 0..passes {
        check_deadline(start, deadline)?;
        for idx in 0..page_count {
            if idx % DEADLINE_CHECK_INTERVAL == 0 {
                check_deadline(start, deadline)?;
            }
            touch_page(buffer, idx, page_size);
        }
    }
    Ok(())
}

fn random_sweep(
    buffer: &mut [u8],
    page_size: usize,
    page_count: usize,
    passes: u64,
    start: Instant,
    deadline: Option<Duration>,
) -> Result<(), RunError> {
    if page_count == 0 {
        return Ok(());
    }

    let mut indices: Vec<usize> = (0..page_count).collect();
    for pass in 0..passes {
        check_deadline(start, deadline)?;
        let seed = (pass + 1) ^ (page_count as u64).wrapping_mul(0x9E3779B97F4A7C15);
        shuffle_indices(&mut indices, seed);
        for (idx, &page) in indices.iter().enumerate() {
            if idx % DEADLINE_CHECK_INTERVAL == 0 {
                check_deadline(start, deadline)?;
            }
            touch_page(buffer, page, page_size);
        }
    }

    Ok(())
}

fn shuffle_indices(indices: &mut [usize], seed: u64) {
    let mut rng = XorShift64::new(seed);
    for i in (1..indices.len()).rev() {
        let j = (rng.next() as usize) % (i + 1);
        indices.swap(i, j);
    }
}

fn touch_page(buffer: &mut [u8], page: usize, page_size: usize) {
    let offset = page.saturating_mul(page_size);
    if let Some(slot) = buffer.get_mut(offset) {
        *slot = slot.wrapping_add(1);
    }
}

fn check_deadline(start: Instant, deadline: Option<Duration>) -> Result<(), RunError> {
    if let Some(limit) = deadline {
        let elapsed = start.elapsed();
        if elapsed >= limit {
            return Err(RunError::Timeout { elapsed, limit });
        }
    }
    Ok(())
}

fn page_count(total_bytes: usize, page_size: usize) -> usize {
    (total_bytes + page_size - 1) / page_size
}

#[cfg(unix)]
fn page_size() -> usize {
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if size <= 0 { 4096 } else { size as usize }
}

#[cfg(not(unix))]
fn page_size() -> usize {
    4096
}

fn parse_positive_u64(raw: &str, flag: &str) -> Result<u64, String> {
    raw.parse::<u64>()
        .map_err(|_| format!("{flag} expects a positive integer, got '{raw}'"))
        .and_then(|value| {
            if value == 0 {
                Err(format!("{flag} must be greater than zero"))
            } else {
                Ok(value)
            }
        })
}

fn parse_positive_usize(raw: &str, flag: &str) -> Result<usize, String> {
    raw.parse::<usize>()
        .map_err(|_| format!("{flag} expects a positive integer, got '{raw}'"))
        .and_then(|value| {
            if value == 0 {
                Err(format!("{flag} must be greater than zero"))
            } else {
                Ok(value)
            }
        })
}

fn print_usage(program: &str, mut target: impl Write) {
    let _ = writeln!(
        target,
        "\
Usage: {program} [--megabytes N] [--passes N] [--pattern sequential|random] [--max-seconds N]\n\n\
Options:\n  --megabytes N   Number of MiB to allocate (default {DEFAULT_MEGABYTES}).\n  --passes N      How many full sweeps to perform (default {DEFAULT_PASSES}).\n  --pattern MODE  Access pattern: sequential or random (default sequential).\n  --max-seconds N Abort after N seconds (self-imposed timeout).\n  --help          Show this message.\n\n\
Exit codes:\n  0 success, 1 invalid arguments, 2 internal error, 3 timeout."
    );
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        let seed = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}
