/// Diagnostic types for sysml-cli validation results.

use crate::model::Span;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Note,
    Warning,
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Note => write!(f, "note"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub file: String,
    pub span: Span,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

impl Diagnostic {
    pub fn error(file: &str, span: Span, code: &'static str, message: String) -> Self {
        Self {
            file: file.to_string(),
            span,
            severity: Severity::Error,
            code,
            message,
        }
    }

    pub fn warning(file: &str, span: Span, code: &'static str, message: String) -> Self {
        Self {
            file: file.to_string(),
            span,
            severity: Severity::Warning,
            code,
            message,
        }
    }

    pub fn note(file: &str, span: Span, code: &'static str, message: String) -> Self {
        Self {
            file: file.to_string(),
            span,
            severity: Severity::Note,
            code,
            message,
        }
    }
}

/// Diagnostic codes used by sysml-cli.
pub mod codes {
    // Errors
    pub const SYNTAX_ERROR: &str = "E001";
    pub const DUPLICATE_DEF: &str = "E002";

    // Warnings
    pub const UNUSED_DEF: &str = "W001";
    pub const UNSATISFIED_REQ: &str = "W002";
    pub const UNVERIFIED_REQ: &str = "W003";
    pub const UNRESOLVED_TYPE: &str = "W004";
    pub const UNRESOLVED_TARGET: &str = "W005";
    pub const PORT_TYPE_MISMATCH: &str = "W006";
    pub const EMPTY_CONSTRAINT: &str = "W007";
    pub const CALC_NO_RETURN: &str = "W008";
}
