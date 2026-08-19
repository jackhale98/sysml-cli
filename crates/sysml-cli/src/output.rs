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

use sysml_core::view_render::RenderedTable;

/// Print a rendered table in the requested format (json/csv/md/text).
/// Shared by `view` and the commands that render through the same table
/// pipeline (`trace`, `allocation`).
pub(crate) fn print_table(format: &str, t: &RenderedTable) {
    match format {
        "json" => println!("{}", serde_json::to_string_pretty(t).unwrap()),
        "csv" => table_csv(t),
        "md" => table_markdown(t),
        _ => table_text(t),
    }
}

fn table_text(t: &RenderedTable) {
    // Column widths from content.
    let mut widths: Vec<usize> = t.columns.iter().map(|c| c.chars().count()).collect();
    for row in &t.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }
    }
    let line = |cells: &[String]| {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{:<w$}", c, w = widths.get(i).copied().unwrap_or(0)))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    println!("{}", line(&t.columns));
    println!(
        "{}",
        "-".repeat(widths.iter().sum::<usize>() + 2 * (widths.len().saturating_sub(1)))
    );
    for row in &t.rows {
        println!("{}", line(row));
    }
    eprintln!("{} row(s).", t.rows.len());
}

fn table_csv(t: &RenderedTable) {
    let esc = |s: &str| {
        if s.contains([',', '"', '\n']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    println!(
        "{}",
        t.columns
            .iter()
            .map(|c| esc(c))
            .collect::<Vec<_>>()
            .join(",")
    );
    for row in &t.rows {
        println!(
            "{}",
            row.iter().map(|c| esc(c)).collect::<Vec<_>>().join(",")
        );
    }
}

fn table_markdown(t: &RenderedTable) {
    let esc = |s: &str| s.replace('|', "\\|");
    println!(
        "| {} |",
        t.columns
            .iter()
            .map(|c| esc(c))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    println!("|{}|", vec!["---"; t.columns.len()].join("|"));
    for row in &t.rows {
        println!(
            "| {} |",
            row.iter().map(|c| esc(c)).collect::<Vec<_>>().join(" | ")
        );
    }
}
