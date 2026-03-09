use std::path::PathBuf;
use std::process::ExitCode;

use crate::read_source;

pub(crate) fn run(
    files: &[PathBuf],
    check: bool,
    show_diff: bool,
    indent_width: usize,
) -> ExitCode {
    use sysml_core::codegen::edit;

    let mut any_unformatted = false;

    for file_path in files {
        let (path_str, source) = match read_source(file_path) {
            Ok(v) => v,
            Err(code) => return code,
        };

        let formatted = format_sysml(&source, indent_width);

        if formatted == source {
            continue;
        }

        any_unformatted = true;

        if check {
            eprintln!("{}: not formatted", path_str);
        } else if show_diff {
            print!("{}", edit::diff(&source, &formatted, &path_str));
        } else {
            if let Err(e) = std::fs::write(file_path, &formatted) {
                eprintln!("error: cannot write `{}`: {}", path_str, e);
                return ExitCode::from(1);
            }
            eprintln!("Formatted {}", path_str);
        }
    }

    if check && any_unformatted {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn format_sysml(source: &str, indent_width: usize) -> String {
    let mut out = String::new();
    let mut depth: usize = 0;
    let indent_str = " ".repeat(indent_width);

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }

        if trimmed.starts_with('}') {
            depth = depth.saturating_sub(1);
        }

        for _ in 0..depth {
            out.push_str(&indent_str);
        }
        out.push_str(trimmed);
        out.push('\n');

        if trimmed.ends_with('{') {
            depth += 1;
        }
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }

    out
}
