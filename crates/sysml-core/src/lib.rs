//! SysML v2 parser, validator, simulator, and model query engine.
//!
//! This crate provides the core logic for working with SysML v2 models.
//! It is frontend-agnostic — no CLI, GUI, or I/O dependencies. All public
//! functions take data in and return data out.
//!
//! # Modules
//!
//! - [`model`] — Data types for SysML v2 model elements (definitions, usages, relationships)
//! - [`parser`] — Tree-sitter based parsing of SysML v2 textual notation
//! - [`resolver`] — Multi-file import resolution
//! - [`diagnostic`] — Diagnostic types and error codes
//! - [`checks`] — Validation checks (lint rules)
//! - [`sim`] — Simulation engine (constraints, state machines, action flows)
//! - [`export`] — Export to FMI, Modelica, and SSP formats

pub mod cache;
pub mod checks;
pub mod codegen;
pub mod config;
pub mod diagram;
pub mod diagnostic;
pub mod export;
pub mod index;
pub mod interactive;
pub mod model;
pub mod parser;
pub mod project;
pub mod qualified_name;
pub mod query;
pub mod record;
pub mod resolver;
pub mod sim;
#[cfg(feature = "sqlite")]
pub mod sqlite_cache;
