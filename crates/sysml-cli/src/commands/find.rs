//! Find command: search model elements by name pattern.

use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;

use crate::Cli;

pub fn run(cli: &Cli, files: &[PathBuf], pattern: &str, kind: &str) -> ExitCode {
    let (files, _) = crate::files_or_project(files);
    if files.is_empty() {
        eprintln!("error: no SysML files found.");
        return ExitCode::FAILURE;
    }

    let show_defs = kind == "all" || kind == "definitions" || kind == "defs";
    let show_usages = kind == "all" || kind == "usages";

    let pat_lower = pattern.to_lowercase();
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut text_lines: Vec<String> = Vec::new();

    for file_path in &files {
        let path_str = file_path.to_string_lossy().to_string();
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let model = sysml_parser::parse_file(&path_str, &source);

        if show_defs {
            for def in &model.definitions {
                let matches = def.name.to_lowercase().contains(&pat_lower)
                    || def
                        .doc
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&pat_lower));
                if matches {
                    text_lines.push(format!(
                        "  {:14} {:30} {}:{}",
                        def.kind.label(),
                        def.name,
                        path_str,
                        def.span.start_row,
                    ));
                    results.push(serde_json::json!({
                        "kind": def.kind.label(),
                        "name": def.name,
                        "file": path_str,
                        "line": def.span.start_row,
                        "parent": def.parent_def,
                    }));
                }
            }
        }

        if show_usages {
            for usage in &model.usages {
                let matches = usage.name.to_lowercase().contains(&pat_lower)
                    || usage
                        .type_ref
                        .as_ref()
                        .is_some_and(|t| t.to_lowercase().contains(&pat_lower));
                if matches {
                    text_lines.push(format!(
                        "  {:14} {:30} {}:{}",
                        usage.kind,
                        format!(
                            "{}{}",
                            usage.name,
                            usage
                                .type_ref
                                .as_ref()
                                .map(|t| format!(" : {}", t))
                                .unwrap_or_default()
                        ),
                        path_str,
                        usage.span.start_row,
                    ));
                    results.push(serde_json::json!({
                        "kind": usage.kind,
                        "name": usage.name,
                        "type_ref": usage.type_ref,
                        "file": path_str,
                        "line": usage.span.start_row,
                        "parent": usage.parent_def,
                    }));
                }
            }
        }
    }

    match cli.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&results).unwrap());
        }
        _ => {
            if text_lines.is_empty() {
                eprintln!("No matches for `{}`.", pattern);
            } else {
                println!("{} match(es) for `{}`:", text_lines.len(), pattern);
                for line in &text_lines {
                    println!("{}", line);
                }
            }
        }
    }

    ExitCode::SUCCESS
}
