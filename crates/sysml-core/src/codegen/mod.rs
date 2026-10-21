//! Code generation and text editing for SysML v2 models.
//!
//! - [`template`] — Generate SysML v2 boilerplate text
//! - [`edit`] — Byte-position text edits using CST spans
//! - [`field_edit`] — Table-cell edits: set one field of one element
//! - [`format`] — CST-aware source formatting

pub mod edit;
pub mod field_edit;
pub mod format;
pub mod template;
