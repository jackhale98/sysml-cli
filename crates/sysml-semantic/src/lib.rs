//! Semantic model for SysML v2 and KerML.
//!
//! This crate is the home of the shared engine's abstract-syntax layer: the
//! element graph that mirrors the KerML/SysML metamodel ([`graph`]), the
//! lowering from the tree-sitter concrete syntax tree ([`lower`]), name
//! resolution ([`resolve`]), implied relationships, element identity
//! ([`id`]) and the queries consumers use ([`Model`]). It has no I/O and
//! builds for `wasm32-unknown-unknown`.

pub mod graph;
pub mod id;
mod implied;
pub(crate) mod lower;
pub mod metaclass;
mod model;
pub mod name;
pub mod resolve;

pub use graph::{
    Direction, Element, ElementRef, End, FileId, Flags, RefId, Role, Span, Target, Value,
    Visibility,
};
pub use id::{ElementId, Spec};
pub use metaclass::Metaclass;
pub use model::{
    BuildStats, Diagnostic, DiagnosticKind, Library, Member, Model, Project, SourceFile,
    DEFAULT_PROJECT_URI,
};
pub use resolve::{Origin, Resolution};
