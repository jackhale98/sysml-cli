use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;

use crate::{read_source, Cli};

#[derive(Serialize)]
struct FmtFileResult {
    file: String,
    action: &'static str,
    would_change: bool,
}

#[derive(Serialize)]
struct FmtEnvelope<'a> {
    command: &'a str,
    files: Vec<FmtFileResult>,
}

pub(crate) fn run(
    cli: &Cli,
    files: &[PathBuf],
    check: bool,
    show_diff: bool,
    indent_width: usize,
) -> ExitCode {
    use sysml_core::codegen::edit;
    use sysml_core::codegen::format::{format_source, FormatOptions};

    let opts = FormatOptions {
        indent_width,
        trailing_newline: true,
    };

    let json_mode = cli.format == "json";
    let mut any_unformatted = false;
    let mut results: Vec<FmtFileResult> = Vec::new();

    for file_path in files {
        let (path_str, source) = match read_source(file_path) {
            Ok(v) => v,
            Err(code) => return code,
        };

        let formatted = format_source(&source, &opts);
        let would_change = formatted != source;

        if !would_change {
            if json_mode {
                results.push(FmtFileResult {
                    file: path_str.clone(),
                    action: "unchanged",
                    would_change: false,
                });
            }
            continue;
        }

        any_unformatted = true;

        if check {
            if json_mode {
                results.push(FmtFileResult {
                    file: path_str.clone(),
                    action: "would-format",
                    would_change: true,
                });
            } else {
                eprintln!("{}: not formatted", path_str);
            }
        } else if show_diff {
            if json_mode {
                results.push(FmtFileResult {
                    file: path_str.clone(),
                    action: "diff",
                    would_change: true,
                });
            } else {
                print!("{}", edit::diff(&source, &formatted, &path_str));
            }
        } else {
            if let Err(e) = std::fs::write(file_path, &formatted) {
                eprintln!("error: cannot write `{}`: {}", path_str, e);
                return ExitCode::from(1);
            }
            if json_mode {
                results.push(FmtFileResult {
                    file: path_str.clone(),
                    action: "formatted",
                    would_change: true,
                });
            } else {
                eprintln!("Formatted {}", path_str);
            }
        }
    }

    if json_mode {
        let envelope = FmtEnvelope {
            command: "fmt",
            files: results,
        };
        match serde_json::to_string_pretty(&envelope) {
            Ok(s) => println!("{}", s),
            Err(e) => {
                eprintln!("error: failed to serialise JSON: {}", e);
                return ExitCode::from(1);
            }
        }
    }

    if check && any_unformatted {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
