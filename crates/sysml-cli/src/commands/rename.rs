/// Top-level `rename` command — rename an element and update all references.
/// With --project, renames across all files in the project.

use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;

use sysml_core::parser as sysml_parser;
use sysml_core::codegen::edit;

use crate::{read_source, Cli};

#[derive(Serialize)]
struct EditSummary {
    file: String,
    edits: usize,
    diff: Option<String>,
}

#[derive(Serialize)]
struct RenameEnvelope<'a> {
    command: &'a str,
    from: &'a str,
    to: &'a str,
    dry_run: bool,
    project_wide: bool,
    files: Vec<EditSummary>,
    edits: usize,
}

pub(crate) fn run(
    cli: &Cli,
    file: &PathBuf,
    old_name: &str,
    new_name: &str,
    dry_run: bool,
    project_wide: bool,
) -> ExitCode {
    if project_wide {
        return run_project_wide(cli, file, old_name, new_name, dry_run);
    }

    // Single-file rename
    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let model = sysml_parser::parse_file(&path_str, &source);

    let plan = match edit::rename_element(&source, &model, old_name, new_name) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };

    let result = match edit::apply_edits(&source, &plan) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };

    let json_mode = cli.format == "json";

    if dry_run {
        if json_mode {
            let envelope = RenameEnvelope {
                command: "rename",
                from: old_name,
                to: new_name,
                dry_run: true,
                project_wide: false,
                files: vec![EditSummary {
                    file: path_str.clone(),
                    edits: plan.edits.len(),
                    diff: Some(edit::diff(&source, &result, &path_str)),
                }],
                edits: plan.edits.len(),
            };
            println!("{}", serde_json::to_string_pretty(&envelope).unwrap_or_default());
        } else {
            print!("{}", edit::diff(&source, &result, &path_str));
        }
    } else {
        if let Err(e) = std::fs::write(file, &result) {
            eprintln!("error: cannot write `{}`: {}", path_str, e);
            return ExitCode::from(1);
        }
        if json_mode {
            let envelope = RenameEnvelope {
                command: "rename",
                from: old_name,
                to: new_name,
                dry_run: false,
                project_wide: false,
                files: vec![EditSummary {
                    file: path_str.clone(),
                    edits: plan.edits.len(),
                    diff: None,
                }],
                edits: plan.edits.len(),
            };
            println!("{}", serde_json::to_string_pretty(&envelope).unwrap_or_default());
        } else {
            eprintln!("Renamed `{}` to `{}` in {}", old_name, new_name, path_str);
        }
    }
    ExitCode::SUCCESS
}

fn run_project_wide(
    cli: &Cli,
    start_file: &PathBuf,
    old_name: &str,
    new_name: &str,
    dry_run: bool,
) -> ExitCode {
    // Discover all project files
    let (files, _) = crate::files_or_project(&[start_file.clone()]);
    if files.is_empty() {
        eprintln!("error: no SysML files found in project.");
        return ExitCode::FAILURE;
    }

    let json_mode = cli.format == "json";
    let mut summaries: Vec<EditSummary> = Vec::new();
    let mut changed_count = 0;
    let mut total_replacements = 0;

    for file_path in &files {
        let path_str = file_path.to_string_lossy().to_string();
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("warning: cannot read `{}`: {}", path_str, e);
                continue;
            }
        };

        let model = sysml_parser::parse_file(&path_str, &source);

        let plan = match edit::rename_element(&source, &model, old_name, new_name) {
            Ok(p) => p,
            Err(_) => continue, // Element not found in this file, skip
        };

        if plan.edits.is_empty() {
            continue;
        }

        let result = match edit::apply_edits(&source, &plan) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("warning: cannot apply edits to `{}`: {}", path_str, e);
                continue;
            }
        };

        if result == source {
            continue;
        }

        let replacements = plan.edits.len();
        total_replacements += replacements;
        changed_count += 1;

        if json_mode {
            summaries.push(EditSummary {
                file: path_str.clone(),
                edits: replacements,
                diff: if dry_run {
                    Some(edit::diff(&source, &result, &path_str))
                } else {
                    None
                },
            });
        }

        if dry_run {
            if !json_mode {
                print!("{}", edit::diff(&source, &result, &path_str));
            }
        } else {
            if let Err(e) = std::fs::write(file_path, &result) {
                eprintln!("error: cannot write `{}`: {}", path_str, e);
                return ExitCode::from(1);
            }
            if !json_mode {
                eprintln!(
                    "  {} ({} replacement{})",
                    path_str,
                    replacements,
                    if replacements == 1 { "" } else { "s" }
                );
            }
        }
    }

    if json_mode {
        let envelope = RenameEnvelope {
            command: "rename",
            from: old_name,
            to: new_name,
            dry_run,
            project_wide: true,
            files: summaries,
            edits: total_replacements,
        };
        println!("{}", serde_json::to_string_pretty(&envelope).unwrap_or_default());
    } else if changed_count == 0 {
        eprintln!("No occurrences of `{}` found in {} file(s).", old_name, files.len());
    } else if !dry_run {
        eprintln!(
            "Renamed `{}` to `{}`: {} replacement(s) across {} file(s).",
            old_name, new_name, total_replacements, changed_count
        );
    }

    ExitCode::SUCCESS
}
