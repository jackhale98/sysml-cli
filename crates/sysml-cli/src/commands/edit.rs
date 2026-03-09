use std::process::ExitCode;

use sysml_core::parser as sysml_parser;

use crate::{EditCommand, read_source, select_item};

pub(crate) fn run(kind: &EditCommand) -> ExitCode {
    use sysml_core::codegen::edit;
    use sysml_core::codegen::template;

    match kind {
        EditCommand::Add {
            file, kind, name, type_ref, inside, dry_run,
        } => {
            let (path_str, source) = match read_source(file) {
                Ok(v) => v,
                Err(code) => return code,
            };
            let model = sysml_parser::parse_file(&path_str, &source);

            // Only generate a full definition template for explicit def kinds
            // (e.g., "part-def", "port def", "package"), not usage keywords like "part"
            let is_def_kind = kind.contains("def") || kind.contains("package")
                || kind.contains("pkg");
            let text = if is_def_kind {
                if let Some(def_kind) = template::parse_template_kind(kind) {
                    let opts = template::TemplateOptions {
                        kind: def_kind,
                        name: name.clone(),
                        super_type: type_ref.clone(),
                        is_abstract: false,
                        short_name: None,
                        doc: None,
                        members: Vec::new(),
                        exposes: Vec::new(),
                        filter: None,
                        indent: if inside.is_some() { 4 } else { 0 },
                    };
                    template::generate_template(&opts)
                } else {
                    eprintln!("error: unknown definition kind `{}`", kind);
                    return ExitCode::from(1);
                }
            } else {
                // Usage format: kind name [: type];
                let t = type_ref
                    .as_ref()
                    .map(|t| format!(" : {}", t))
                    .unwrap_or_default();
                format!("{} {}{};", kind, name, t)
            };

            // Determine where to insert
            let target_parent: Option<String> = if let Some(parent) = inside {
                Some(parent.clone())
            } else if !is_def_kind {
                // For usage-level elements, prompt for which definition to insert into
                let defs_with_body: Vec<&str> = model.definitions.iter()
                    .filter(|d| d.body_end_byte.is_some())
                    .map(|d| d.name.as_str())
                    .collect();
                if defs_with_body.len() == 1 {
                    Some(defs_with_body[0].to_string())
                } else if defs_with_body.len() > 1 {
                    match select_item("definition", &defs_with_body) {
                        Some(idx) => Some(defs_with_body[idx].to_string()),
                        None => return ExitCode::from(1),
                    }
                } else {
                    None // No definitions with bodies — insert top-level
                }
            } else {
                None
            };

            let text_edit = if let Some(ref parent) = target_parent {
                match edit::insert_member(&source, &model, parent, text.trim()) {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("error: {}", e);
                        return ExitCode::from(1);
                    }
                }
            } else {
                edit::insert_top_level(&source, text.trim())
            };

            let result = match edit::apply_edits(&source, &edit::EditPlan { edits: vec![text_edit] }) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {}", e);
                    return ExitCode::from(1);
                }
            };

            if *dry_run {
                print!("{}", edit::diff(&source, &result, &path_str));
            } else {
                if let Err(e) = std::fs::write(file, &result) {
                    eprintln!("error: cannot write `{}`: {}", path_str, e);
                    return ExitCode::from(1);
                }
                eprintln!("Added `{}` to {}", name, path_str);
            }
            ExitCode::SUCCESS
        }
        EditCommand::Remove { file, name, dry_run } => {
            let (path_str, source) = match read_source(file) {
                Ok(v) => v,
                Err(code) => return code,
            };
            let model = sysml_parser::parse_file(&path_str, &source);

            let text_edit = match edit::remove_element(&source, &model, name) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("error: {}", e);
                    return ExitCode::from(1);
                }
            };

            let result = match edit::apply_edits(&source, &edit::EditPlan { edits: vec![text_edit] }) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {}", e);
                    return ExitCode::from(1);
                }
            };

            if *dry_run {
                print!("{}", edit::diff(&source, &result, &path_str));
            } else {
                if let Err(e) = std::fs::write(file, &result) {
                    eprintln!("error: cannot write `{}`: {}", path_str, e);
                    return ExitCode::from(1);
                }
                eprintln!("Removed `{}` from {}", name, path_str);
            }
            ExitCode::SUCCESS
        }
        EditCommand::Rename { file, old_name, new_name, dry_run } => {
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

            if *dry_run {
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
    }
}
