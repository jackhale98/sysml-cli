/// Model file writing utilities with tree-sitter validation.

use std::io;
use std::path::Path;

use sysml_core::codegen::edit;
use sysml_core::codegen::template::validate_generated;
use sysml_core::parser as sysml_parser;

/// Write generated SysML text to a model file.
///
/// If `inside` is provided, inserts the text as a member of that definition.
/// Otherwise appends at the top level.
///
/// Validates the generated text through tree-sitter before writing.
pub fn write_to_model(file: &Path, sysml_text: &str, inside: Option<&str>) -> io::Result<()> {
    // Validate generated text
    if let Err(errors) = validate_generated(sysml_text) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Generated SysML has syntax errors:\n  {}", errors.join("\n  ")),
        ));
    }

    let source = std::fs::read_to_string(file)?;
    let path_str = file.to_string_lossy();
    let model = sysml_parser::parse_file(&path_str, &source);

    let text_edit = if let Some(parent) = inside {
        edit::insert_member(&source, &model, parent, sysml_text.trim())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?
    } else {
        edit::insert_top_level(&source, sysml_text.trim())
    };

    let result = edit::apply_edits(&source, &edit::EditPlan { edits: vec![text_edit] })
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    std::fs::write(file, result)
}

/// Interactively select a target file from .sysml files in the model root.
pub fn select_target_file(model_root: &Path) -> Option<std::path::PathBuf> {
    use dialoguer::FuzzySelect;
    use std::io::IsTerminal;

    if !std::io::stderr().is_terminal() {
        return None;
    }

    let mut files = Vec::new();
    collect_sysml_files(model_root, &mut files);

    if files.is_empty() {
        eprintln!("No .sysml files found in {}", model_root.display());
        return None;
    }

    let labels: Vec<String> = files
        .iter()
        .map(|f| f.strip_prefix(model_root).unwrap_or(f).to_string_lossy().to_string())
        .collect();

    match FuzzySelect::new()
        .with_prompt("Select target file")
        .items(&labels)
        .default(0)
        .interact_opt()
    {
        Ok(Some(idx)) => Some(files[idx].clone()),
        _ => None,
    }
}

/// Interactively select a parent definition from a file.
pub fn select_parent_def(file: &Path) -> Option<String> {
    use dialoguer::FuzzySelect;
    use std::io::IsTerminal;

    if !std::io::stderr().is_terminal() {
        return None;
    }

    let source = std::fs::read_to_string(file).ok()?;
    let model = sysml_parser::parse_file(&file.to_string_lossy(), &source);

    let defs_with_body: Vec<&str> = model.definitions.iter()
        .filter(|d| d.body_end_byte.is_some())
        .map(|d| d.name.as_str())
        .collect();

    if defs_with_body.is_empty() {
        return None;
    }

    let mut items: Vec<&str> = vec!["(top-level)"];
    items.extend(defs_with_body.iter());

    match FuzzySelect::new()
        .with_prompt("Insert inside which definition?")
        .items(&items)
        .default(0)
        .interact_opt()
    {
        Ok(Some(0)) => None, // top-level
        Ok(Some(idx)) => Some(items[idx].to_string()),
        _ => None,
    }
}

fn collect_sysml_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_sysml_files(&path, files);
            } else if let Some(ext) = path.extension() {
                if ext == "sysml" {
                    files.push(path);
                }
            }
        }
    }
}
