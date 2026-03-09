use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::checks::{self, Check};
use sysml_core::diagnostic::{Diagnostic, Severity};
use sysml_core::parser as sysml_parser;

use crate::{Cli, collect_files_recursive};
use crate::output;

pub(crate) fn run(cli: &Cli, files: &[PathBuf], disable: &[String], severity: &str) -> ExitCode {
    let disabled: HashSet<&str> = disable.iter().map(|s| s.as_str()).collect();
    let min_severity = match severity {
        "error" => Severity::Error,
        "warning" => Severity::Warning,
        _ => Severity::Note,
    };

    let active_checks: Vec<Box<dyn Check>> = checks::all_checks()
        .into_iter()
        .filter(|c| !disabled.contains(c.name()))
        .collect();

    // Build project resolver if includes are specified
    let project = if !cli.include.is_empty() {
        let mut all_files: Vec<PathBuf> = files.to_vec();
        for inc in &cli.include {
            if inc.is_dir() {
                collect_files_recursive(inc, &mut all_files);
            } else {
                all_files.push(inc.clone());
            }
        }
        Some(sysml_core::resolver::Project::from_files(&all_files))
    } else if files.len() > 1 {
        // Multi-file lint: auto-resolve imports between the given files
        Some(sysml_core::resolver::Project::from_files(files))
    } else {
        None
    };

    let mut all_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut had_parse_error = false;

    for file_path in files {
        let path_str = file_path.to_string_lossy().to_string();

        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read `{}`: {}", path_str, e);
                had_parse_error = true;
                continue;
            }
        };

        let mut model = sysml_parser::parse_file(&path_str, &source);

        // Resolve imports if project is available
        if let Some(ref proj) = project {
            model.resolved_imports = proj.resolve_imports(&model);
        }

        for check in &active_checks {
            let diagnostics = check.run(&model);
            for d in diagnostics {
                if d.severity >= min_severity {
                    all_diagnostics.push(d);
                }
            }
        }
    }

    all_diagnostics.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.span.start_row.cmp(&b.span.start_row))
            .then(a.span.start_col.cmp(&b.span.start_col))
    });

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

    let has_errors = all_diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);

    if has_errors || had_parse_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
