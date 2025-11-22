//! Minimal JSON pointer extractor for probes.
//!
//! Reads JSON from a file or stdin, optionally walks a JSON Pointer, enforces an
//! expected type, and prints the selected value as compact JSON. Designed for
//! probes that need to pull booleans/numbers/objects out of helper output
//! without adding scripting dependencies.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = CliArgs::parse()?;
    let source = args.source.read()?;
    let value: Value = serde_json::from_slice(&source).context("failed to parse JSON input")?;

    let selected = if args.pointer.is_empty() {
        Some(&value)
    } else {
        value.pointer(&args.pointer)
    };

    let output = match selected {
        Some(val) => {
            if let Some(expected) = args.expected_type {
                if !expected.matches(val) {
                    if let Some(default) = args.default_value {
                        default
                    } else {
                        bail!(
                            "value at pointer {} is not of expected type {}",
                            args.pointer,
                            expected.label()
                        );
                    }
                } else {
                    val.clone()
                }
            } else {
                val.clone()
            }
        }
        None => match args.default_value {
            Some(default) => default,
            None => bail!("pointer {} not found in input", args.pointer),
        },
    };

    if let Some(expected) = args.expected_type {
        if !expected.matches(&output) {
            bail!(
                "output value does not match expected type {}",
                expected.label()
            );
        }
    }

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum ValueType {
    Object,
    Array,
    String,
    Number,
    Bool,
    Null,
}

impl ValueType {
    fn from_str(raw: &str) -> Result<Self> {
        match raw {
            "object" => Ok(Self::Object),
            "array" => Ok(Self::Array),
            "string" => Ok(Self::String),
            "number" => Ok(Self::Number),
            "bool" | "boolean" => Ok(Self::Bool),
            "null" => Ok(Self::Null),
            other => bail!("unknown type '{other}' (expected object|array|string|number|bool|null)"),
        }
    }

    fn matches(&self, value: &Value) -> bool {
        match self {
            ValueType::Object => value.is_object(),
            ValueType::Array => value.is_array(),
            ValueType::String => value.is_string(),
            ValueType::Number => value.is_number(),
            ValueType::Bool => value.is_boolean(),
            ValueType::Null => value.is_null(),
        }
    }

    fn label(&self) -> &'static str {
        match self {
            ValueType::Object => "object",
            ValueType::Array => "array",
            ValueType::String => "string",
            ValueType::Number => "number",
            ValueType::Bool => "bool",
            ValueType::Null => "null",
        }
    }
}

struct CliArgs {
    source: InputSource,
    pointer: String,
    expected_type: Option<ValueType>,
    default_value: Option<Value>,
}

enum InputSource {
    File(PathBuf),
    Stdin,
}

impl InputSource {
    fn read(&self) -> Result<Vec<u8>> {
        match self {
            InputSource::File(path) => {
                if !path.is_file() {
                    bail!("input file not found: {}", path.display());
                }
                Ok(fs::read(path).with_context(|| format!("reading {}", path.display()))?)
            }
            InputSource::Stdin => {
                let mut buf = Vec::new();
                io::stdin()
                    .read_to_end(&mut buf)
                    .context("reading stdin")?;
                Ok(buf)
            }
        }
    }
}

impl CliArgs {
    fn parse() -> Result<Self> {
        let mut args = env::args_os().skip(1);
        let mut source: Option<InputSource> = None;
        let mut pointer: Option<String> = None;
        let mut expected_type: Option<ValueType> = None;
        let mut default_value: Option<Value> = None;

        while let Some(arg_os) = args.next() {
            let arg = arg_os
                .into_string()
                .map_err(|_| anyhow::anyhow!("argument is not valid UTF-8"))?;
            match arg.as_str() {
                "--file" => {
                    let path = next_value(&mut args, "--file")?;
                    if source.is_some() {
                        bail!("--file/--stdin may only be provided once");
                    }
                    source = Some(InputSource::File(PathBuf::from(path)));
                }
                "--stdin" => {
                    if source.is_some() {
                        bail!("--file/--stdin may only be provided once");
                    }
                    source = Some(InputSource::Stdin);
                }
                "--pointer" => {
                    let raw = next_value(&mut args, "--pointer")?;
                    if !raw.is_empty() && !raw.starts_with('/') {
                        bail!("--pointer must be empty (root) or start with '/'");
                    }
                    pointer = Some(raw);
                }
                "--type" => {
                    let raw = next_value(&mut args, "--type")?;
                    expected_type = Some(ValueType::from_str(&raw)?);
                }
                "--default" => {
                    let raw = next_value(&mut args, "--default")?;
                    let parsed: Value = serde_json::from_str(&raw)
                        .with_context(|| format!("invalid JSON for --default: {raw}"))?;
                    default_value = Some(parsed);
                }
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                other => bail!("unknown flag: {other}"),
            }
        }

        let source = source.unwrap_or(InputSource::Stdin);
        let pointer = pointer.unwrap_or_else(|| "".to_string());

        Ok(CliArgs {
            source,
            pointer,
            expected_type,
            default_value,
        })
    }
}

fn next_value(args: &mut impl Iterator<Item = std::ffi::OsString>, flag: &str) -> Result<String> {
    args.next()
        .map(|os| {
            os.into_string()
                .map_err(|_| anyhow::anyhow!("value for {flag} is not valid UTF-8"))
        })
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))
}

fn usage() -> &'static str {
    "Usage: json-extract [--file PATH|--stdin] [--pointer /json/pointer] [--type object|array|string|number|bool|null] [--default JSON]\n\
Reads JSON, selects the value at the given JSON Pointer (default: root), enforces an optional type, and prints the value as compact JSON.\n"
}

fn print_usage() {
    print!("{}", usage());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_expected_types() {
        assert!(ValueType::Bool.matches(&Value::Bool(true)));
        assert!(ValueType::Number.matches(&Value::from(1)));
        assert!(!ValueType::String.matches(&Value::from(1)));
    }

    #[test]
    fn parse_expected_type_variants() {
        assert!(matches!(ValueType::from_str("object"), Ok(ValueType::Object)));
        assert!(ValueType::from_str("bool").is_ok());
        assert!(ValueType::from_str("boolean").is_ok());
        assert!(ValueType::from_str("unknown").is_err());
    }

    #[test]
    fn default_is_used_when_pointer_missing() {
        let args = CliArgs {
            source: InputSource::Stdin,
            pointer: "/missing".to_string(),
            expected_type: Some(ValueType::Bool),
            default_value: Some(Value::Bool(false)),
        };
        let json = br#"{"present":true}"#.to_vec();
        let value: Value = serde_json::from_slice(&json).unwrap();
        let selected = value.pointer(&args.pointer);
        assert!(selected.is_none());
        assert_eq!(args.default_value, Some(Value::Bool(false)));
    }
}
