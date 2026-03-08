/// sysml-lint: SysML v2 model validator and linter.
///
/// Uses tree-sitter to parse SysML v2 files and runs structural
/// validation checks. Designed for CI pipelines and editor integration.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use sysml_lint::checks::{self, Check};
use sysml_lint::diagnostic::{Diagnostic, Severity};
use sysml_lint::output;
use sysml_lint::parser as sysml_parser;

#[derive(Parser)]
#[command(
    name = "sysml-lint",
    about = "SysML v2 model validator and linter",
    version
)]
struct Cli {
    /// SysML v2 files to validate.
    #[arg(required = true)]
    files: Vec<PathBuf>,

    /// Output format: text, json.
    #[arg(short, long, default_value = "text")]
    format: String,

    /// Disable specific checks (comma-separated).
    /// Available: syntax, duplicates, unused, unresolved, unsatisfied, unverified, port-types, constraints, calculations
    #[arg(short, long, value_delimiter = ',')]
    disable: Vec<String>,

    /// Minimum severity to report: note, warning, error.
    #[arg(short, long, default_value = "note")]
    severity: String,

    /// Suppress summary line on stderr.
    #[arg(short, long)]
    quiet: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let disabled: HashSet<&str> = cli.disable.iter().map(|s| s.as_str()).collect();
    let min_severity = match cli.severity.as_str() {
        "error" => Severity::Error,
        "warning" => Severity::Warning,
        _ => Severity::Note,
    };

    // Build check list, filtering disabled checks
    let active_checks: Vec<Box<dyn Check>> = checks::all_checks()
        .into_iter()
        .filter(|c| !disabled.contains(c.name()))
        .collect();

    let mut all_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut had_parse_error = false;

    for file_path in &cli.files {
        let path_str = file_path.to_string_lossy().to_string();

        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read `{}`: {}", path_str, e);
                had_parse_error = true;
                continue;
            }
        };

        let model = sysml_parser::parse_file(&path_str, &source);

        for check in &active_checks {
            let diagnostics = check.run(&model);
            for d in diagnostics {
                if d.severity >= min_severity {
                    all_diagnostics.push(d);
                }
            }
        }
    }

    // Sort by file, then line, then column
    all_diagnostics.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.span.start_row.cmp(&b.span.start_row))
            .then(a.span.start_col.cmp(&b.span.start_col))
    });

    // Output
    if !all_diagnostics.is_empty() {
        let output = match cli.format.as_str() {
            "json" => output::format_json(&all_diagnostics),
            _ => output::format_text(&all_diagnostics),
        };
        println!("{}", output);
    }

    if !cli.quiet {
        output::print_summary(&all_diagnostics);
    }

    // Exit code
    let has_errors = all_diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);

    if has_errors || had_parse_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
