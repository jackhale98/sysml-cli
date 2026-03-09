use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;

use crate::read_source;

pub(crate) fn run(
    kind: &str,
    name: &str,
    output: Option<&PathBuf>,
    extends: Option<&str>,
    is_abstract: bool,
    short_name: Option<&str>,
    doc: Option<&str>,
    members: &[String],
    exposes: &[String],
    filter: Option<&str>,
    append: bool,
    inside: Option<&str>,
    dry_run: bool,
) -> ExitCode {
    use sysml_core::codegen::template::*;
    use sysml_core::codegen::edit;

    let def_kind = match parse_template_kind(kind) {
        Some(k) => k,
        None => {
            eprintln!("error: unknown element kind `{}`", kind);
            eprintln!("  available: part-def, port-def, action-def, state-def, constraint-def,");
            eprintln!("            calc-def, requirement, enum-def, attribute-def, item-def,");
            eprintln!("            view-def, viewpoint-def, package, use-case, connection-def,");
            eprintln!("            flow-def, interface-def, allocation-def");
            return ExitCode::from(1);
        }
    };

    let parsed_members: Vec<MemberSpec> = members
        .iter()
        .filter_map(|s| parse_member_spec(s))
        .collect();

    let indent = if inside.is_some() { 4 } else { 0 };

    let opts = TemplateOptions {
        kind: def_kind,
        name: name.to_string(),
        super_type: extends.map(|s| s.to_string()),
        is_abstract,
        short_name: short_name.map(|s| s.to_string()),
        doc: doc.map(|s| s.to_string()),
        members: parsed_members,
        exposes: exposes.to_vec(),
        filter: filter.map(|s| s.to_string()),
        indent,
    };

    let generated = generate_template(&opts);

    if let Some(parent_name) = inside {
        let file = match output {
            Some(f) => f,
            None => {
                eprintln!("error: --inside requires --output <file>");
                return ExitCode::from(1);
            }
        };
        let (path_str, source) = match read_source(file) {
            Ok(v) => v,
            Err(code) => return code,
        };
        let model = sysml_parser::parse_file(&path_str, &source);
        let text_edit = match edit::insert_member(&source, &model, parent_name, generated.trim()) {
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
        if dry_run {
            print!("{}", edit::diff(&source, &result, &path_str));
        } else {
            if let Err(e) = std::fs::write(file, &result) {
                eprintln!("error: cannot write `{}`: {}", path_str, e);
                return ExitCode::from(1);
            }
            eprintln!("Added {} inside {} in {}", name, parent_name, path_str);
        }
        return ExitCode::SUCCESS;
    }

    if append {
        if let Some(file) = output {
            let (path_str, source) = match read_source(file) {
                Ok(v) => v,
                Err(code) => return code,
            };
            let text_edit = edit::insert_top_level(&source, generated.trim());
            let result = match edit::apply_edits(&source, &edit::EditPlan { edits: vec![text_edit] }) {
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
                eprintln!("Appended {} to {}", name, path_str);
            }
            return ExitCode::SUCCESS;
        }
    }

    if dry_run {
        print!("{}", generated);
    } else if let Some(file) = output {
        if let Err(e) = std::fs::write(file, &generated) {
            eprintln!("error: cannot write `{}`: {}", file.display(), e);
            return ExitCode::from(1);
        }
        eprintln!("Wrote {} to {}", name, file.display());
    } else {
        print!("{}", generated);
    }

    ExitCode::SUCCESS
}
