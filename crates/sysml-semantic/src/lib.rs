//! Semantic model for SysML v2 and KerML.
//!
//! This crate is the home of the shared engine's abstract-syntax layer: the
//! element graph that mirrors the KerML/SysML metamodel, element identity,
//! and (in later phases) name resolution, implied relationships and
//! validation. It has no I/O and builds for `wasm32-unknown-unknown`.
//!
//! Phase 0 provides [`id`]: normative element identifiers (KerML 1.0, 9.1)
//! and the `path()` / `qualifiedName` / `escapedName()` derivations they
//! depend on, verified against every element of the OMG standard library.

pub mod id;
pub mod name;

pub use id::{ElementId, Spec};
