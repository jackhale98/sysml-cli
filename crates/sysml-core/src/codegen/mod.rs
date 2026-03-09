/// Code generation and text editing for SysML v2 models.
///
/// - [`template`] — Generate SysML v2 boilerplate text
/// - [`edit`] — Byte-position text edits using CST spans
/// - [`format`] — CST-aware source formatting

pub mod edit;
pub mod format;
pub mod template;
