//! Top-level `remove` command — remove an element from a SysML file.

use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;

use sysml_core::codegen::edit;
use sysml_core::parser as sysml_parser;

use crate::{read_source, Cli};

#[derive(Serialize)]
struct RemoveResult<'a> {
    command: &'a str,
    file: String,
    removed: &'a str,
    dry_run: bool,
    bytes_removed: usize,
    diff: Option<String>,
}

pub(crate) fn run(cli: &Cli, file: &PathBuf, name: &str, dry_run: bool) -> ExitCode {
    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let model = sysml_parser::parse_file(&path_str, &source);

    let text_edit = match edit::remove_element(&source, &model, name) {
        Ok(e) => e,
        Err(e) => {
            if cli.format == "json" {
                let err = serde_json::json!({
                    "command": "remove",
                    "file": path_str,
                    "removed": name,
                    "error": e.to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&err).unwrap_or_default());
            } else {
                eprintln!("error: {}", e);
            }
            return ExitCode::from(1);
        }
    };

    let result = match edit::apply_edits(
        &source,
        &edit::EditPlan {
            edits: vec![text_edit],
        },
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };

    let bytes_removed = source.len().saturating_sub(result.len());
    let json_mode = cli.format == "json";

    if dry_run {
        if json_mode {
            let envelope = RemoveResult {
                command: "remove",
                file: path_str.clone(),
                removed: name,
                dry_run: true,
                bytes_removed,
                diff: Some(edit::diff(&source, &result, &path_str)),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&envelope).unwrap_or_default()
            );
        } else {
            print!("{}", edit::diff(&source, &result, &path_str));
        }
    } else {
        if let Err(e) = std::fs::write(file, &result) {
            eprintln!("error: cannot write `{}`: {}", path_str, e);
            return ExitCode::from(1);
        }
        if json_mode {
            let envelope = RemoveResult {
                command: "remove",
                file: path_str.clone(),
                removed: name,
                dry_run: false,
                bytes_removed,
                diff: None,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&envelope).unwrap_or_default()
            );
        } else {
            eprintln!("Removed `{}` from {}", name, path_str);
        }
    }
    ExitCode::SUCCESS
}
