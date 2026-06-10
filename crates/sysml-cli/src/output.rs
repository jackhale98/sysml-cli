//! Output formatting for diagnostics and simulation results.

use sysml_core::diagnostic::{Diagnostic, Severity};

/// Format diagnostics as human-readable text.
pub fn format_text(diagnostics: &[Diagnostic]) -> String {
    let mut lines = Vec::new();
    for d in diagnostics {
        lines.push(format!(
            "{}:{}:{}: {}[{}]: {}",
            d.file, d.span.start_row, d.span.start_col, d.severity, d.code, d.message,
        ));
        if let Some(ref suggestion) = d.suggestion {
            lines.push(format!("  help: {}", suggestion));
        }
    }
    lines.join("\n")
}

/// Format diagnostics as JSON array.
pub fn format_json(diagnostics: &[Diagnostic]) -> String {
    serde_json::to_string_pretty(diagnostics).unwrap_or_else(|_| "[]".to_string())
}

/// Print a summary line to stderr.
pub fn print_summary(diagnostics: &[Diagnostic]) {
    let errors = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    let notes = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Note)
        .count();

    if errors + warnings + notes == 0 {
        eprintln!("No issues found.");
    } else {
        let parts: Vec<String> = [(errors, "error"), (warnings, "warning"), (notes, "note")]
            .iter()
            .filter(|(count, _)| *count > 0)
            .map(|(count, label)| {
                if *count == 1 {
                    format!("{} {}", count, label)
                } else {
                    format!("{} {}s", count, label)
                }
            })
            .collect();

        eprintln!("Found {}.", parts.join(", "));
    }
}
