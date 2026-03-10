/// Top-level `rename` command — rename an element and update all references.

use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;
use sysml_core::codegen::edit;

use crate::read_source;

pub(crate) fn run(
    file: &PathBuf,
    old_name: &str,
    new_name: &str,
    dry_run: bool,
) -> ExitCode {
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

    if dry_run {
        print!("{}", edit::diff(&source, &result, &path_str));
    } else {
        if let Err(e) = std::fs::write(file, &result) {
            eprintln!("error: cannot write `{}`: {}", path_str, e);
            return ExitCode::from(1);
        }
        eprintln!("Renamed `{}` to `{}` in {}", old_name, new_name, path_str);
    }
    ExitCode::SUCCESS
}
