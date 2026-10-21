//! Model interchange for SysML v2 and KerML (KerML 1.0, Clause 10).
//!
//! Phase 0 provides project interchange files ([`kpar`]); JSON and XMI
//! model serialization build on the semantic graph in later phases. No I/O:
//! everything works on byte slices so it runs in the browser too.

pub mod kpar;

pub use kpar::{
    Checksum, ChecksumProblem, Checksums, Kpar, KparError, ProjectInfo, ProjectMeta, ProjectUsage,
};
