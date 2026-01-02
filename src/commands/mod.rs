//! Internal subcommands used by the runner-owned script helpers.
//!
//! These are not part of the user-facing CLI. The only public entry point is
//! `fencerunner [--strict|--supervised] <RUN_DIR>...`.

pub mod commit_help_me;
pub mod emit_record;
