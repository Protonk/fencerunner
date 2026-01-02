//! Boundary-object types and helpers.
//!
//! Re-exports the boundary types and run-dir boundary contract loader so
//! callers can import from `boundary::*` without hunting for files.

pub mod contract;
pub mod types;

pub use contract::{BoundaryContract, BoundaryContractIndex, BoundaryStdout};
pub use types::{
    BoundaryObject, CommitmentEnrollment, ContextInfo, OperationInfo, ProbeInfo, ResultDetails,
    ResultInfo, RunInfo, StackInfo,
};
