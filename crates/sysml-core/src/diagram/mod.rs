//! Diagram generation for SysML v2 models.
//!
//! This module provides a format-agnostic intermediate representation
//! (`DiagramGraph`) and builders for each SysML v2 diagram type.
//! The graph is then rendered by format-specific emitters (Mermaid,
//! PlantUML, DOT) in the CLI crate.

mod builders;
mod emitters;
mod graph;

pub use builders::*;
pub use emitters::*;
pub use graph::*;
