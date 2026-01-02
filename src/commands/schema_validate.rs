//! Internal schema validation helper.
//!
//! This is used by tests and by script-facing shims to validate run-dir-local
//! contracts and boundary records without relying on repo-relative paths.

use crate::boundary::BoundaryContractIndex;
use crate::commitments::index::CommitmentIndex;
use crate::gates::contract::GatesContractIndex;
use crate::repo_tools::boundaries_contract_path;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, stdin};
use std::path::PathBuf;

pub fn run(args: &[std::ffi::OsString]) -> Result<()> {
    let cli = Cli::parse(args)?;

    match cli.mode.as_str() {
        "gates" => {
            let Some(path) = cli.file else {
                bail!("--file is required for --mode gates");
            };
            GatesContractIndex::load(&path)
                .with_context(|| format!("loading {}", path.display()))?;
        }
        "commitments" => {
            let Some(path) = cli.file else {
                bail!("--file is required for --mode commitments");
            };
            CommitmentIndex::load(&path).with_context(|| format!("loading {}", path.display()))?;
        }
        "boundaries-contract" => {
            let Some(path) = cli.file else {
                bail!("--file is required for --mode boundaries-contract");
            };
            BoundaryContractIndex::load(&path)
                .with_context(|| format!("loading {}", path.display()))?;
        }
        "boundary" => {
            if cli.run_dir.is_some() && cli.contract.is_some() {
                bail!("cannot combine --run-dir and --contract");
            }
            let input = read_input(cli.file)?;
            let contract_path = if let Some(path) = cli.contract {
                path
            } else if let Some(run_dir) = cli.run_dir {
                boundaries_contract_path(&run_dir)
            } else {
                bail!("boundary mode requires --contract or --run-dir");
            };
            let contract = BoundaryContractIndex::load(&contract_path)
                .with_context(|| format!("loading {}", contract_path.display()))?;
            contract
                .validate_record(&input)
                .with_context(|| format!("record violates {}", contract_path.display()))?;
        }
        other => bail!("unknown mode '{}'", other),
    }

    Ok(())
}

fn read_input(file: Option<PathBuf>) -> Result<Value> {
    let mut buf = String::new();
    if let Some(path) = file {
        File::open(&path)
            .with_context(|| format!("opening input file {}", path.display()))?
            .read_to_string(&mut buf)
            .with_context(|| format!("reading input file {}", path.display()))?;
    } else {
        stdin()
            .read_to_string(&mut buf)
            .context("reading stdin for input JSON")?;
    }
    let value: Value = serde_json::from_str(&buf).context("parsing input JSON")?;
    Ok(value)
}

struct Cli {
    mode: String,
    file: Option<PathBuf>,
    run_dir: Option<PathBuf>,
    contract: Option<PathBuf>,
}

impl Cli {
    fn parse(args: &[std::ffi::OsString]) -> Result<Self> {
        let mut args = args.iter().cloned();

        let mut mode: Option<String> = None;
        let mut file: Option<PathBuf> = None;
        let mut run_dir: Option<PathBuf> = None;
        let mut contract: Option<PathBuf> = None;

        while let Some(arg_os) = args.next() {
            let arg = arg_os
                .to_str()
                .ok_or_else(|| anyhow!("invalid UTF-8 in argument"))?;

            if arg == "--help" || arg == "-h" {
                usage(0);
            }

            if let Some((flag, value)) = arg.split_once('=') {
                match flag {
                    "--mode" => mode = Some(value.to_string()),
                    "--file" => file = Some(PathBuf::from(value)),
                    "--run-dir" => run_dir = Some(PathBuf::from(value)),
                    "--contract" => contract = Some(PathBuf::from(value)),
                    _ => bail!("unknown option: {flag}"),
                }
                continue;
            }

            match arg {
                "--mode" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --mode"))?;
                    mode = Some(os_to_string(value)?);
                }
                "--file" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --file"))?;
                    file = Some(PathBuf::from(value));
                }
                "--run-dir" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --run-dir"))?;
                    run_dir = Some(PathBuf::from(value));
                }
                "--contract" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --contract"))?;
                    contract = Some(PathBuf::from(value));
                }
                other if other.starts_with('-') => bail!("unknown option: {other}"),
                other => bail!("unexpected positional argument: {other}"),
            }
        }

        let mode = mode.unwrap_or_else(|| {
            eprintln!("Missing required option: --mode");
            usage(1);
        });

        match mode.as_str() {
            "gates" | "commitments" | "boundaries-contract" | "boundary" => {}
            other => bail!(
                "unknown mode '{}' (expected gates|commitments|boundaries-contract|boundary)",
                other
            ),
        }

        Ok(Self {
            mode,
            file,
            run_dir,
            contract,
        })
    }
}

fn usage(code: i32) -> ! {
    eprintln!(
        "Usage:\n  schema-validate --mode gates --file <gates.json>\n  schema-validate --mode commitments --file <commitments.json>\n  schema-validate --mode boundaries-contract --file <boundaries.json>\n  schema-validate --mode boundary [--run-dir <RUN_DIR> | --contract <boundaries.json>] [--file <record.json>]\n  schema-validate --mode boundary [--run-dir <RUN_DIR> | --contract <boundaries.json>] < record.json\n\nOptions:\n  --mode <MODE>            gates|commitments|boundaries-contract|boundary\n  --file <PATH>            Input JSON file (stdin when omitted in boundary mode)\n  --run-dir <DIR>          Use <DIR>/boundaries.json for boundary validation\n  --contract <PATH>        Use explicit boundaries.json for boundary validation\n  -h, --help               Show this help text."
    );
    std::process::exit(code);
}

fn os_to_string(value: std::ffi::OsString) -> Result<String> {
    value
        .into_string()
        .map_err(|_| anyhow!("invalid UTF-8 in argument"))
}
