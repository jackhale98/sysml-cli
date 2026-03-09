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
    use sysml_core::codegen::format::{format_source, FormatOptions};

    let opts = FormatOptions {
        indent_width,
        trailing_newline: true,
    };

    let mut any_unformatted = false;

    for file_path in files {
        let (path_str, source) = match read_source(file_path) {
            Ok(v) => v,
            Err(code) => return code,
        };

        let formatted = format_source(&source, &opts);

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
