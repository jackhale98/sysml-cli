/// Enhanced validation command that combines lint checks with project-level
/// record integrity checking.
///
/// When `lint_only` is true, behaves identically to `sysml lint`.  When false,
/// additionally discovers the enclosing project (via `.sysml/config.toml`) and
/// validates that TOML records in the configured output directory have valid
/// `[refs]` entries that correspond to model elements that still exist.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::checks::{self, Check};
use sysml_core::diagnostic::{Diagnostic, Severity};
use sysml_core::model::Span;
use sysml_core::parser as sysml_parser;
use sysml_core::record::RecordEnvelope;

use crate::{Cli, collect_files_recursive, resolve_include_paths};
use crate::output;

pub fn run(
    cli: &Cli,
    files: &[PathBuf],
    disable: &[String],
    severity: &str,
    lint_only: bool,
) -> ExitCode {
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

    // Build project resolver if includes or stdlib are specified
    let effective_includes = resolve_include_paths(cli);
    let project = if !effective_includes.is_empty() {
        let mut all_files: Vec<PathBuf> = files.to_vec();
        for inc in &effective_includes {
            if inc.is_dir() {
                collect_files_recursive(inc, &mut all_files);
            } else {
                all_files.push(inc.clone());
            }
        }
        Some(sysml_core::resolver::Project::from_files(&all_files))
    } else if files.len() > 1 {
        Some(sysml_core::resolver::Project::from_files(files))
    } else {
        None
    };

    let mut all_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut had_parse_error = false;

    // Collect all model element names across files for record validation.
    let mut all_element_names: HashSet<String> = HashSet::new();

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

        // Collect element names for record cross-referencing
        if !lint_only {
            for def in &model.definitions {
                all_element_names.insert(def.name.clone());
                // Also register qualified names so refs like "Vehicle::Engine"
                // can match.
                if let Some(ref qn) = def.qualified_name {
                    all_element_names.insert(qn.to_string());
                }
            }
            for usage in &model.usages {
                all_element_names.insert(usage.name.clone());
                if let Some(ref qn) = usage.qualified_name {
                    all_element_names.insert(qn.to_string());
                }
            }
        }
    }

    // --- Project-level record checks (only when not lint-only) ---
    if !lint_only {
        if let Some(start_path) = files.first() {
            if let Some((project_root, config)) =
                sysml_core::project::discover_project(start_path)
            {
                let records_dir = project_root.join(&config.defaults.output_dir);
                if records_dir.is_dir() {
                    check_records(
                        &records_dir,
                        &all_element_names,
                        min_severity,
                        &mut all_diagnostics,
                    );
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

/// Diagnostic code for broken record references.
const RECORD_BROKEN_REF: &str = "R001";
/// Diagnostic code for orphaned records (all refs point to missing elements).
const RECORD_ORPHANED: &str = "R002";

/// Scan `.toml` files in `records_dir` and emit diagnostics for broken or
/// orphaned record references.
fn check_records(
    records_dir: &PathBuf,
    element_names: &HashSet<String>,
    min_severity: Severity,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let entries = match std::fs::read_dir(records_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "warning: cannot read records directory `{}`: {}",
                records_dir.display(),
                e
            );
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Only inspect .toml files
        let is_toml = path
            .extension()
            .map_or(false, |ext| ext == "toml");
        if !is_toml || !path.is_file() {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();

        let content = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("warning: cannot read record `{}`: {}", path_str, e);
                continue;
            }
        };

        let envelope = match RecordEnvelope::from_toml_str(&content) {
            Ok(env) => env,
            Err(e) => {
                // Malformed record — emit as a warning (not every .toml
                // in the directory is necessarily a record).
                let d = Diagnostic::warning(
                    &path_str,
                    Span::default(),
                    RECORD_BROKEN_REF,
                    format!("cannot parse record: {}", e),
                );
                if d.severity >= min_severity {
                    diagnostics.push(d);
                }
                continue;
            }
        };

        // Check each ref entry against known model elements.
        let mut total_refs: usize = 0;
        let mut broken_refs: usize = 0;

        for (role, names) in &envelope.refs {
            for name in names {
                total_refs += 1;
                if !ref_matches_element(name, element_names) {
                    broken_refs += 1;
                    let d = Diagnostic::warning(
                        &path_str,
                        Span::default(),
                        RECORD_BROKEN_REF,
                        format!(
                            "broken reference in [refs].{}: `{}` does not match any model element",
                            role, name
                        ),
                    )
                    .with_suggestion(format!(
                        "Update or remove `{}` from the record, or add the missing element to the model.",
                        name
                    ));
                    if d.severity >= min_severity {
                        diagnostics.push(d);
                    }
                }
            }
        }

        // If *all* refs are broken, the record is orphaned.
        if total_refs > 0 && broken_refs == total_refs {
            let d = Diagnostic::warning(
                &path_str,
                Span::default(),
                RECORD_ORPHANED,
                format!(
                    "orphaned record `{}`: all {} reference(s) point to missing model elements",
                    envelope.meta.id, total_refs
                ),
            )
            .with_explanation(
                "This record's model references no longer match any element in the \
                 analyzed files. It may be left over from a deleted or renamed element."
                    .to_string(),
            )
            .with_suggestion(
                "Delete the record file or update its [refs] to reference current model elements."
                    .to_string(),
            );
            if d.severity >= min_severity {
                diagnostics.push(d);
            }
        }
    }
}

/// Check whether a record reference name matches any known model element.
///
/// Handles both simple names (`Engine`) and qualified names
/// (`Vehicle::Engine`).  A qualified ref matches if the full string is in
/// `element_names` **or** if the final segment (simple name) matches.  This
/// provides reasonable behaviour when records use qualified names but the
/// check is run on a subset of model files.
fn ref_matches_element(ref_name: &str, element_names: &HashSet<String>) -> bool {
    // Exact match (simple or qualified).
    if element_names.contains(ref_name) {
        return true;
    }

    // Fall back to matching the simple (last-segment) name.
    let simple = ref_name
        .rsplit("::")
        .next()
        .unwrap_or(ref_name);
    element_names.contains(simple)
}
