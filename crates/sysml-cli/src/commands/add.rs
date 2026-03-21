/// Unified `add` command — creates SysML elements interactively or with flags.

use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::parser as sysml_parser;
use sysml_core::codegen::{edit, template};

use crate::{read_source, select_item};

/// Dispatch add command based on argument combinations.
///
/// | file | kind | name | --stdout | Behavior                      |
/// |------|------|------|----------|-------------------------------|
/// | None | None | None | false    | Full interactive wizard       |
/// | None | Some | Some | *        | Stdout (infer --stdout)       |
/// | Some | None | None | false    | Guided: parse file, wizard    |
/// | Some | Some | Some | false    | Direct insert into file       |
///
/// Special kinds: `connection` (with --connect), `satisfy`/`verify` (with --by),
/// `import` (name is the import path).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    file: Option<&PathBuf>,
    kind: Option<&str>,
    name: Option<&str>,
    type_ref: Option<&str>,
    inside: Option<&str>,
    dry_run: bool,
    stdout: bool,
    teach: bool,
    doc: Option<&str>,
    extends: Option<&str>,
    is_abstract: bool,
    short_name: Option<&str>,
    members: &[String],
    exposes: &[String],
    filter: Option<&str>,
    _interactive: bool,
    connect: Option<&str>,
    satisfy: Option<&str>,
    verify: Option<&str>,
    by: Option<&str>,
) -> ExitCode {
    // Handle --satisfy/--verify flags (no positional kind needed)
    if let Some(req) = satisfy {
        let by_elem = match by {
            Some(b) => b,
            None => {
                eprintln!("error: --satisfy requires --by <element>");
                return ExitCode::from(1);
            }
        };
        let text = template::generate_relationship("satisfy", req, by_elem, 0);
        return handle_generated_text(file, &text, inside, dry_run, stdout, "satisfy");
    }
    if let Some(req) = verify {
        let by_elem = match by {
            Some(b) => b,
            None => {
                eprintln!("error: --verify requires --by <element>");
                return ExitCode::from(1);
            }
        };
        let text = template::generate_relationship("verify", req, by_elem, 0);
        return handle_generated_text(file, &text, inside, dry_run, stdout, "verify");
    }

    // Reinterpret positionals: clap fills file/kind/name in order.
    // When --stdout is set and `file` looks like a kind (not a path), shift args.
    let (eff_file, eff_kind, eff_name) = if stdout || teach {
        match (file, kind, name) {
            (Some(f), Some(k), None) => {
                (None, Some(f.to_string_lossy().to_string()), Some(k.to_string()))
            }
            (Some(f), None, None) => {
                (None, Some(f.to_string_lossy().to_string()), None)
            }
            _ => (
                file.cloned(),
                kind.map(|s| s.to_string()),
                name.map(|s| s.to_string()),
            ),
        }
    } else {
        (
            file.cloned(),
            kind.map(|s| s.to_string()),
            name.map(|s| s.to_string()),
        )
    };

    let eff_file_ref = eff_file.as_ref();
    let eff_kind_ref = eff_kind.as_deref();
    let eff_name_ref = eff_name.as_deref();

    match (eff_file_ref, eff_kind_ref, eff_name_ref) {
        // No args → interactive wizard
        (None, None, None) => {
            run_wizard_mode()
        }
        // No file but kind+name → stdout mode
        (None, Some(kind), Some(name)) => {
            run_stdout(kind, name, extends, is_abstract, short_name, doc,
                       members, exposes, filter, teach, type_ref, connect, by)
        }
        // File but no kind/name → guided file mode
        (Some(file), None, None) if !stdout => {
            run_wizard_mode_with_file(file)
        }
        // File + kind + name → direct insert
        (Some(file), Some(kind), Some(name)) => {
            if stdout {
                run_stdout(kind, name, extends, is_abstract, short_name, doc,
                           members, exposes, filter, teach, type_ref, connect, by)
            } else {
                run_insert(file, kind, name, type_ref, inside, dry_run,
                           doc, extends, is_abstract, short_name, members, connect, by)
            }
        }
        // Partial args
        _ => {
            eprintln!("error: provide either no args (wizard), --stdout <kind> <name>, or <file> <kind> <name>");
            ExitCode::from(1)
        }
    }
}

/// Handle generated text: print to stdout or insert into file.
fn handle_generated_text(
    file: Option<&PathBuf>,
    text: &str,
    inside: Option<&str>,
    dry_run: bool,
    stdout: bool,
    label: &str,
) -> ExitCode {
    if stdout || file.is_none() {
        print!("{}", text);
        return ExitCode::SUCCESS;
    }
    let file = file.unwrap();
    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let model = sysml_parser::parse_file(&path_str, &source);

    let text_edit = if let Some(parent) = inside {
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

    if dry_run {
        print!("{}", edit::diff(&source, &result, &path_str));
    } else {
        if let Err(e) = std::fs::write(file, &result) {
            eprintln!("error: cannot write `{}`: {}", path_str, e);
            return ExitCode::from(1);
        }
        eprintln!("Added {} to {}", label, path_str);
    }
    ExitCode::SUCCESS
}

/// Print generated SysML to stdout (replaces old `new` command).
fn run_stdout(
    kind: &str,
    name: &str,
    extends: Option<&str>,
    is_abstract: bool,
    short_name: Option<&str>,
    doc: Option<&str>,
    members: &[String],
    exposes: &[String],
    filter: Option<&str>,
    teach: bool,
    type_ref: Option<&str>,
    connect: Option<&str>,
    by: Option<&str>,
) -> ExitCode {
    // For teach mode, delegate to scaffold
    if teach {
        #[cfg(feature = "scaffold")]
        {
            let options = sysml_scaffold::ScaffoldOptions {
                extends: extends.map(|s| s.to_string()),
                doc: doc.map(|s| s.to_string()),
                members: Vec::new(),
                with_teaching_comments: true,
            };
            match sysml_scaffold::scaffold_element(kind, name, &options) {
                Ok(text) => {
                    print!("{}", text);
                    return ExitCode::SUCCESS;
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    return ExitCode::FAILURE;
                }
            }
        }
        #[cfg(not(feature = "scaffold"))]
        {
            eprintln!("error: --teach requires the 'scaffold' feature (install with --features scaffold)");
            return ExitCode::FAILURE;
        }
    }

    // Handle special kinds: import, satisfy, verify, connection with --connect
    match kind {
        "import" => {
            print!("{}", template::generate_import(name, 0));
            return ExitCode::SUCCESS;
        }
        "satisfy" => {
            if let Some(target) = by.or(type_ref).or(extends) {
                print!("{}", template::generate_relationship("satisfy", name, target, 0));
            } else {
                eprintln!("error: satisfy requires --by <element> (or -t <element>)");
                return ExitCode::from(1);
            }
            return ExitCode::SUCCESS;
        }
        "verify" => {
            if let Some(target) = by.or(type_ref).or(extends) {
                print!("{}", template::generate_relationship("verify", name, target, 0));
            } else {
                eprintln!("error: verify requires --by <element> (or -t <element>)");
                return ExitCode::from(1);
            }
            return ExitCode::SUCCESS;
        }
        "connection" if connect.is_some() => {
            print!("{}", template::generate_connection_usage(
                name, type_ref, connect.unwrap(), 0));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    // Check if this is a definition kind
    let is_def_kind = kind.contains("def") || kind.contains("package")
        || kind.contains("pkg") || kind == "requirement" || kind == "req";

    if is_def_kind {
        let def_kind = match template::parse_template_kind(kind) {
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

        let parsed_members = parse_members(members, kind);

        let super_type = extends.or(type_ref);

        let opts = template::TemplateOptions {
            kind: def_kind,
            name: name.to_string(),
            super_type: super_type.map(|s| s.to_string()),
            is_abstract,
            short_name: short_name.map(|s| s.to_string()),
            doc: doc.map(|s| s.to_string()),
            members: parsed_members,
            exposes: exposes.to_vec(),
            filter: filter.map(|s| s.to_string()),
            indent: 0,
        };

        let generated = template::generate_template(&opts);
        print!("{}", generated);
    } else {
        // Usage format: kind name [: type];
        let t = type_ref
            .map(|t| format!(" : {}", t))
            .unwrap_or_default();
        println!("{} {}{};", kind, name, t);
    }

    ExitCode::SUCCESS
}

/// Parse member specs, with special handling for enum-def (bare names become enum members).
fn parse_members(members: &[String], kind: &str) -> Vec<template::MemberSpec> {
    let is_enum = kind == "enum-def" || kind == "enum";
    members
        .iter()
        .filter_map(|s| {
            if is_enum {
                // For enums, bare names like "red" become enum members
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    return None;
                }
                // If it already has a space (like "enum red"), parse normally
                if trimmed.contains(' ') {
                    return template::parse_member_spec(trimmed);
                }
                // Otherwise treat as a bare enum member name
                Some(template::MemberSpec {
                    usage_kind: "enum".to_string(),
                    name: trimmed.to_string(),
                    type_ref: None,
                    direction: None,
                    multiplicity: None,
                    raw_line: false,
                })
            } else {
                template::parse_member_spec(s)
            }
        })
        .collect()
}

/// Concept-first interactive wizard for `sysml add` with no arguments.
///
/// Flow: select file → parse model context → choose what to create →
/// fill in details with model-aware prompts → preview → write.
fn run_wizard_mode() -> ExitCode {
    use sysml_core::interactive::*;
    use crate::wizard::CliWizardRunner;

    let runner = CliWizardRunner::new();
    if !runner.is_interactive() {
        eprintln!("error: interactive wizard requires a terminal");
        eprintln!("Usage: sysml add <file> <kind> <name>");
        return ExitCode::from(1);
    }

    // Step 1: Choose destination first so we can parse the model context
    let dest_step = WizardStep::choice(
        "destination",
        "Where will this element go?",
        vec![
            ("file", "Add to an existing file (model-aware)"),
            ("stdout", "Print to terminal (no file context)"),
        ],
    ).with_explanation("File mode shows available types from your model.");

    let dest = match runner.run_step(&dest_step) {
        Some(WizardAnswer::String(s)) => s,
        _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
    };

    // Parse model context if a file is selected
    let (target_file, model) = if dest == "file" {
        let target = match crate::model_writer::select_target_file(std::path::Path::new(".")) {
            Some(p) => p,
            None => {
                eprintln!("No .sysml files found. Switching to stdout mode.");
                return run_wizard_with_context(&runner, None, None);
            }
        };
        let model = parse_file_context(&target);
        (Some(target), model)
    } else {
        (None, None)
    };

    run_wizard_with_context(&runner, target_file.as_ref(), model.as_ref())
}

/// Guided file mode: `sysml add <file>` with no kind/name.
fn run_wizard_mode_with_file(file: &PathBuf) -> ExitCode {
    use sysml_core::interactive::WizardRunner;
    use crate::wizard::CliWizardRunner;

    let runner = CliWizardRunner::new();
    if !runner.is_interactive() {
        eprintln!("error: guided mode requires an interactive terminal");
        eprintln!("Usage: sysml add <file> <kind> <name>");
        return ExitCode::from(1);
    }

    let model = parse_file_context(file);
    run_wizard_with_context(&runner, Some(file), model.as_ref())
}

/// Parse a file and collect all .sysml files in the same directory + project
/// library paths for type context.
fn parse_file_context(file: &PathBuf) -> Option<sysml_core::model::Model> {
    let (path_str, source) = read_source(file).ok()?;
    let mut model = sysml_parser::parse_file(&path_str, &source);

    // Collect sibling .sysml files in the same directory
    let mut extra_files = Vec::new();
    if let Some(parent_dir) = file.parent() {
        crate::collect_files_recursive(&parent_dir.to_path_buf(), &mut extra_files);
    }

    // Also include project library paths (from .sysml/config.toml)
    for lib_path in crate::resolve_project_includes() {
        crate::collect_files_recursive(&lib_path, &mut extra_files);
    }

    // Parse all extra files for type context
    for extra in &extra_files {
        if extra == file { continue; }
        if let Ok((ext_path, ext_source)) = read_source(extra) {
            let ext_model = sysml_parser::parse_file(&ext_path, &ext_source);
            model.definitions.extend(ext_model.definitions.into_iter());
            model.usages.extend(ext_model.usages.into_iter());
        }
    }

    Some(model)
}


/// Get definition names from the model that match a given DefKind.
fn model_type_options(
    model: &sysml_core::model::Model,
    usage_kind: &str,
) -> Vec<String> {
    use sysml_core::model::DefKind;
    let target = match usage_kind {
        "part" => Some(DefKind::Part),
        "port" => Some(DefKind::Port),
        "action" => Some(DefKind::Action),
        "state" => Some(DefKind::State),
        "attribute" => Some(DefKind::Attribute),
        "item" => Some(DefKind::Item),
        "connection" => Some(DefKind::Connection),
        _ => None,
    };
    match target {
        Some(dk) => model.definitions.iter()
            .filter(|d| d.kind == dk)
            .map(|d| d.name.clone())
            .collect(),
        None => Vec::new(),
    }
}

/// Get requirement definition names from the model.
fn model_requirement_options(model: &sysml_core::model::Model) -> Vec<String> {
    use sysml_core::model::DefKind;
    model.definitions.iter()
        .filter(|d| d.kind == DefKind::Requirement)
        .map(|d| d.name.clone())
        .collect()
}

/// Get all non-requirement definition names suitable as satisfy/verify targets.
fn model_satisfying_options(model: &sysml_core::model::Model) -> Vec<String> {
    use sysml_core::model::DefKind;
    model.definitions.iter()
        .filter(|d| matches!(d.kind,
            DefKind::Part | DefKind::Action | DefKind::Constraint |
            DefKind::Verification | DefKind::UseCase
        ))
        .map(|d| d.name.clone())
        .collect()
}

/// Get verification case names from the model.
fn model_verification_options(model: &sysml_core::model::Model) -> Vec<String> {
    use sysml_core::model::DefKind;
    model.definitions.iter()
        .filter(|d| d.kind == DefKind::Verification)
        .map(|d| d.name.clone())
        .collect()
}

/// Get port usages from the model grouped by parent, formatted as "parent.port".
fn model_port_endpoints(model: &sysml_core::model::Model) -> Vec<String> {
    model.usages.iter()
        .filter(|u| u.kind == "port")
        .filter_map(|u| {
            u.parent_def.as_ref().map(|p| format!("{}.{}", p, u.name))
        })
        .collect()
}

/// Get definition names suitable as supertypes for a given definition kind.
fn model_supertype_options(
    model: &sysml_core::model::Model,
    kind: &str,
) -> Vec<String> {
    use sysml_core::model::DefKind;
    let target = match kind {
        "part-def" => Some(DefKind::Part),
        "port-def" => Some(DefKind::Port),
        "action-def" => Some(DefKind::Action),
        "state-def" => Some(DefKind::State),
        "attribute-def" | "attr" => Some(DefKind::Attribute),
        "requirement" | "req" => Some(DefKind::Requirement),
        "constraint-def" => Some(DefKind::Constraint),
        "calc-def" => Some(DefKind::Calc),
        "enum-def" => Some(DefKind::Enum),
        "item-def" => Some(DefKind::Item),
        _ => None,
    };
    match target {
        Some(dk) => model.definitions.iter()
            .filter(|d| d.kind == dk)
            .map(|d| d.name.clone())
            .collect(),
        None => Vec::new(),
    }
}

/// Core wizard logic shared between no-args and file modes.
fn run_wizard_with_context(
    runner: &crate::wizard::CliWizardRunner,
    target_file: Option<&PathBuf>,
    model: Option<&sysml_core::model::Model>,
) -> ExitCode {
    use sysml_core::interactive::*;

    // Show file context if available
    if let Some(m) = model {
        let def_names: Vec<&str> = m.definitions.iter()
            .take(15)
            .map(|d| d.name.as_str())
            .collect();
        if !def_names.is_empty() {
            let suffix = if m.definitions.len() > 15 {
                format!(" (+{} more)", m.definitions.len() - 15)
            } else {
                String::new()
            };
            eprintln!("Available types: {}{}", def_names.join(", "), suffix);
        }
    }

    // Step: What are you creating?
    let concept_step = WizardStep::choice(
        "concept",
        "What are you creating?",
        vec![
            ("part-def", "Part definition (component type)"),
            ("port-def", "Port definition (interface point)"),
            ("action-def", "Action definition (behavior)"),
            ("state-def", "State machine definition"),
            ("requirement", "Requirement"),
            ("constraint-def", "Constraint definition"),
            ("calc-def", "Calculation definition"),
            ("enum-def", "Enumeration"),
            ("attribute-def", "Attribute definition (data type)"),
            ("connection-def", "Connection definition"),
            ("verification-def", "Verification case"),
            ("part", "Part usage (instance)"),
            ("port", "Port usage"),
            ("attribute", "Attribute usage"),
            ("action", "Action step"),
            ("connection", "Connection (wiring)"),
            ("satisfy", "Satisfy relationship"),
            ("verify", "Verify relationship"),
            ("import", "Import statement"),
            ("package", "Package"),
            ("other", "Other (manual kind entry)"),
        ],
    );

    let kind = match runner.run_step(&concept_step) {
        Some(WizardAnswer::String(s)) => s,
        _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
    };

    // If "other", ask for the kind string
    let kind = if kind == "other" {
        let kind_step = WizardStep::string("custom_kind", "SysML kind (e.g. interface-def, flow-def)");
        match runner.run_step(&kind_step) {
            Some(WizardAnswer::String(s)) => s,
            _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
        }
    } else {
        kind
    };

    // Handle special kinds that have their own wizard flow
    match kind.as_str() {
        "import" => {
            let path_step = WizardStep::string("import_path", "Import path (e.g., Vehicles::* or Sensors::Temp)")
                .with_explanation("Use :: for nesting, * for wildcard.");
            let path = match runner.run_step(&path_step) {
                Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
            };
            let sysml_text = template::generate_import(&path, 0);
            return finish_wizard(runner, target_file, &sysml_text, &path, false);
        }
        "satisfy" | "verify" => {
            // Suggest requirement names from model
            let req = if let Some(m) = model {
                let reqs = model_requirement_options(m);
                if !reqs.is_empty() {
                    let mut choices: Vec<ChoiceOption> = Vec::new();
                    for r in &reqs {
                        choices.push(ChoiceOption { value: r.clone(), label: r.clone(), description: None });
                    }
                    choices.push(ChoiceOption {
                        value: "__custom__".into(), label: "(other)".into(),
                        description: Some("Enter name manually".into()),
                    });
                    let step = WizardStep {
                        id: "req_name".into(),
                        prompt: "Which requirement?".into(),
                        explanation: None,
                        kind: PromptKind::Choice(choices),
                        required: true,
                        default: None,
                    };
                    match runner.run_step(&step) {
                        Some(WizardAnswer::String(s)) if s == "__custom__" => {
                            let custom = WizardStep::string("req_custom", "Requirement name");
                            match runner.run_step(&custom) {
                                Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                                _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
                            }
                        }
                        Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                        _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
                    }
                } else {
                    let step = WizardStep::string("req_name", "Requirement name");
                    match runner.run_step(&step) {
                        Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                        _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
                    }
                }
            } else {
                let step = WizardStep::string("req_name", "Requirement name");
                match runner.run_step(&step) {
                    Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                    _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
                }
            };

            // Suggest target elements from model
            let by = if let Some(m) = model {
                let targets = if kind == "verify" {
                    model_verification_options(m)
                } else {
                    model_satisfying_options(m)
                };
                if !targets.is_empty() {
                    let mut choices: Vec<ChoiceOption> = Vec::new();
                    for t in &targets {
                        choices.push(ChoiceOption { value: t.clone(), label: t.clone(), description: None });
                    }
                    choices.push(ChoiceOption {
                        value: "__custom__".into(), label: "(other)".into(),
                        description: Some("Enter name manually".into()),
                    });
                    let prompt = if kind == "verify" {
                        "Verified by which verification case?"
                    } else {
                        "Satisfied by which element?"
                    };
                    let step = WizardStep {
                        id: "by_element".into(),
                        prompt: prompt.into(),
                        explanation: None,
                        kind: PromptKind::Choice(choices),
                        required: true,
                        default: None,
                    };
                    match runner.run_step(&step) {
                        Some(WizardAnswer::String(s)) if s == "__custom__" => {
                            let custom = WizardStep::string("by_custom", "Element name");
                            match runner.run_step(&custom) {
                                Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                                _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
                            }
                        }
                        Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                        _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
                    }
                } else {
                    let step = WizardStep::string("by_element", "Satisfied/verified by which element?");
                    match runner.run_step(&step) {
                        Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                        _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
                    }
                }
            } else {
                let step = WizardStep::string("by_element", "Satisfied/verified by which element?");
                match runner.run_step(&step) {
                    Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                    _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
                }
            };

            let sysml_text = template::generate_relationship(&kind, &req, &by, 0);
            return finish_wizard(runner, target_file, &sysml_text, &req, false);
        }
        "connection" => {
            let name_step = WizardStep::string("conn_name", "Connection name");
            let name = match runner.run_step(&name_step) {
                Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
            };

            // Suggest connection types from model
            let conn_type = if let Some(m) = model {
                let conn_defs = model_type_options(m, "connection");
                if !conn_defs.is_empty() {
                    let mut choices: Vec<ChoiceOption> = vec![
                        ChoiceOption { value: "".into(), label: "(none)".into(), description: None },
                    ];
                    for cd in &conn_defs {
                        choices.push(ChoiceOption { value: cd.clone(), label: cd.clone(), description: None });
                    }
                    choices.push(ChoiceOption {
                        value: "__custom__".into(), label: "(other)".into(),
                        description: Some("Enter type manually".into()),
                    });
                    let step = WizardStep {
                        id: "conn_type".into(),
                        prompt: "Connection type?".into(),
                        explanation: None,
                        kind: PromptKind::Choice(choices),
                        required: false,
                        default: None,
                    };
                    match runner.run_step(&step) {
                        Some(WizardAnswer::String(s)) if s == "__custom__" => {
                            let custom = WizardStep::string("conn_type_custom", "Type name");
                            match runner.run_step(&custom) {
                                Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
                                _ => None,
                            }
                        }
                        Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
                        _ => None,
                    }
                } else {
                    let step = WizardStep::string("conn_type", "Connection type? (Enter to skip)").optional();
                    match runner.run_step(&step) {
                        Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
                        _ => None,
                    }
                }
            } else {
                let step = WizardStep::string("conn_type", "Connection type? (Enter to skip)").optional();
                match runner.run_step(&step) {
                    Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
                    _ => None,
                }
            };

            // Show available port endpoints
            let endpoints = if let Some(m) = model {
                let ports = model_port_endpoints(m);
                if !ports.is_empty() {
                    eprintln!("Available ports: {}", ports.join(", "));
                }
                let step = WizardStep::string("endpoints", "Connect endpoints (e.g., a.portOut to b.portIn)");
                match runner.run_step(&step) {
                    Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                    _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
                }
            } else {
                let step = WizardStep::string("endpoints", "Connect endpoints (e.g., a.portOut to b.portIn)");
                match runner.run_step(&step) {
                    Some(WizardAnswer::String(s)) if !s.is_empty() => s,
                    _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
                }
            };

            let sysml_text = template::generate_connection_usage(
                &name, conn_type.as_deref(), &endpoints, 0);
            return finish_wizard(runner, target_file, &sysml_text, &name, true);
        }
        _ => {}
    }

    let is_def = kind.contains("def") || kind.contains("package") || kind.contains("pkg")
        || kind == "requirement";

    // Name
    let name_step = WizardStep::string("name", "Name");
    let name = match runner.run_step(&name_step) {
        Some(WizardAnswer::String(s)) if !s.is_empty() => s,
        _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
    };

    // Doc (optional)
    let doc_step = WizardStep::string("doc", "Brief description (Enter to skip)").optional();
    let doc = match runner.run_step(&doc_step) {
        Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    };

    // For definitions — extends? (with model-aware choices)
    let extends = if is_def {
        wizard_extends_prompt(runner, model, &kind)
    } else {
        None
    };

    // For usages — type reference (with model-aware choices)
    let type_ref = if !is_def {
        wizard_type_ref_prompt(runner, model, &kind)
    } else {
        None
    };

    // For enum-def — prompt for enum members
    let enum_members = if kind == "enum-def" {
        let members_step = WizardStep::string("enum_members", "Enum members (comma-separated, e.g., red,green,blue)")
            .with_explanation("Comma-separated list.").optional();
        match runner.run_step(&members_step) {
            Some(WizardAnswer::String(s)) if !s.is_empty() => {
                s.split(',')
                    .map(|m| template::MemberSpec {
                        usage_kind: "enum".to_string(),
                        name: m.trim().to_string(),
                        type_ref: None,
                        direction: None,
                        multiplicity: None,
                        raw_line: false,
                    })
                    .filter(|m| !m.name.is_empty())
                    .collect()
            }
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };

    // Generate SysML text
    let sysml_text = if is_def {
        if let Some(def_kind) = template::parse_template_kind(&kind) {
            let opts = template::TemplateOptions {
                kind: def_kind,
                name: name.clone(),
                super_type: extends.clone(),
                is_abstract: false,
                short_name: None,
                doc: doc.clone(),
                members: enum_members,
                exposes: Vec::new(),
                filter: None,
                indent: 0,
            };
            template::generate_template(&opts)
        } else {
            eprintln!("error: unknown definition kind `{}`", kind);
            return ExitCode::FAILURE;
        }
    } else {
        let t = type_ref.as_deref()
            .map(|t| format!(" : {}", t))
            .unwrap_or_default();
        format!("{} {}{};", kind, name, t)
    };

    // Preview
    eprintln!("\nPreview:");
    for line in sysml_text.lines() {
        eprintln!("  {}", line);
    }
    eprintln!();

    // Write
    if let Some(target) = target_file {
        // For usages, ask which definition to insert inside
        let inside = if !is_def {
            crate::model_writer::select_parent_def(target)
        } else {
            None
        };

        match crate::model_writer::write_to_model(target, &sysml_text, inside.as_deref()) {
            Ok(()) => {
                eprintln!("Wrote {} to {}", name, target.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::FAILURE
            }
        }
    } else {
        println!("{}", sysml_text);
        ExitCode::SUCCESS
    }
}

/// Finish wizard: preview generated text and write to file or stdout.
fn finish_wizard(
    _runner: &crate::wizard::CliWizardRunner,
    target_file: Option<&PathBuf>,
    sysml_text: &str,
    label: &str,
    is_usage: bool,
) -> ExitCode {
    eprintln!("\nPreview:");
    for line in sysml_text.lines() {
        eprintln!("  {}", line);
    }
    eprintln!();

    if let Some(target) = target_file {
        let inside = if is_usage {
            crate::model_writer::select_parent_def(target)
        } else {
            None
        };

        match crate::model_writer::write_to_model(target, sysml_text, inside.as_deref()) {
            Ok(()) => {
                eprintln!("Wrote {} to {}", label, target.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::FAILURE
            }
        }
    } else {
        println!("{}", sysml_text);
        ExitCode::SUCCESS
    }
}

/// Prompt for extends/supertype with model-aware choices.
fn wizard_extends_prompt(
    runner: &crate::wizard::CliWizardRunner,
    model: Option<&sysml_core::model::Model>,
    kind: &str,
) -> Option<String> {
    use sysml_core::interactive::*;

    if let Some(m) = model {
        let supertypes = model_supertype_options(m, kind);
        if !supertypes.is_empty() {
            let mut choice_options: Vec<ChoiceOption> = vec![
                ChoiceOption { value: "".into(), label: "(none)".into(), description: Some("No supertype".into()) },
            ];
            for st in &supertypes {
                choice_options.push(ChoiceOption { value: st.clone(), label: st.clone(), description: None });
            }
            choice_options.push(ChoiceOption {
                value: "__custom__".into(), label: "(other)".into(),
                description: Some("Enter a type name manually".into()),
            });
            let step = WizardStep {
                id: "extends".into(),
                prompt: format!("Extend another {} type?", kind.replace("-def", "")),
                explanation: None,
                kind: PromptKind::Choice(choice_options),
                required: false,
                default: None,
            };
            return match runner.run_step(&step) {
                Some(WizardAnswer::String(s)) if s == "__custom__" => {
                    let custom = WizardStep::string("extends_custom", "Type name");
                    match runner.run_step(&custom) {
                        Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
                        _ => None,
                    }
                }
                Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
                _ => None,
            };
        }
    }
    let step = WizardStep::string("extends", "Extend another type? (Enter to skip)").optional();
    match runner.run_step(&step) {
        Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// Prompt for type reference with model-aware choices.
fn wizard_type_ref_prompt(
    runner: &crate::wizard::CliWizardRunner,
    model: Option<&sysml_core::model::Model>,
    kind: &str,
) -> Option<String> {
    use sysml_core::interactive::*;

    if let Some(m) = model {
        let available_types = model_type_options(m, kind);
        if !available_types.is_empty() {
            let mut choice_options: Vec<ChoiceOption> = vec![
                ChoiceOption { value: "".into(), label: "(none)".into(), description: Some("No type reference".into()) },
            ];
            for t in &available_types {
                choice_options.push(ChoiceOption { value: t.clone(), label: t.clone(), description: None });
            }
            choice_options.push(ChoiceOption {
                value: "__custom__".into(), label: "(other)".into(),
                description: Some("Enter a type name manually".into()),
            });
            let step = WizardStep {
                id: "type_ref".into(),
                prompt: format!("Type for this {} usage?", kind),
                explanation: None,
                kind: PromptKind::Choice(choice_options),
                required: false,
                default: None,
            };
            return match runner.run_step(&step) {
                Some(WizardAnswer::String(s)) if s == "__custom__" => {
                    let custom = WizardStep::string("type_custom", "Type name");
                    match runner.run_step(&custom) {
                        Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
                        _ => None,
                    }
                }
                Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
                _ => None,
            };
        }
    }
    let step = WizardStep::string("type_ref", "Type reference? (Enter to skip)")
        .optional();
    match runner.run_step(&step) {
        Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// Insert element into a file (replaces old `edit add` command).
#[allow(clippy::too_many_arguments)]
fn run_insert(
    file: &PathBuf,
    kind: &str,
    name: &str,
    type_ref: Option<&str>,
    inside: Option<&str>,
    dry_run: bool,
    doc: Option<&str>,
    extends: Option<&str>,
    is_abstract: bool,
    short_name: Option<&str>,
    members: &[String],
    connect: Option<&str>,
    by: Option<&str>,
) -> ExitCode {
    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let model = sysml_parser::parse_file(&path_str, &source);

    // Handle special kinds.
    // Always use indent=0 — insert_member handles indentation for nested insertion.
    let text = match kind {
        "import" => {
            template::generate_import(name, 0)
        }
        "satisfy" => {
            let target = by.or(type_ref).or(extends).unwrap_or("TODO");
            template::generate_relationship("satisfy", name, target, 0)
        }
        "verify" => {
            let target = by.or(type_ref).or(extends).unwrap_or("TODO");
            template::generate_relationship("verify", name, target, 0)
        }
        "connection" if connect.is_some() => {
            template::generate_connection_usage(name, type_ref, connect.unwrap(), 0)
        }
        _ => {
            // Standard definition or usage generation
            let is_def_kind = kind.contains("def") || kind.contains("package")
                || kind.contains("pkg") || kind == "requirement" || kind == "req";
            if is_def_kind {
                match template::parse_template_kind(kind) {
                    Some(def_kind) => {
                        let super_type = extends.or(type_ref).map(|s| s.to_string());
                        let parsed_members = parse_members(members, kind);
                        let opts = template::TemplateOptions {
                            kind: def_kind,
                            name: name.to_string(),
                            super_type,
                            is_abstract,
                            short_name: short_name.map(|s| s.to_string()),
                            doc: doc.map(|s| s.to_string()),
                            members: parsed_members,
                            exposes: Vec::new(),
                            filter: None,
                            indent: 0,
                        };
                        template::generate_template(&opts)
                    }
                    None => {
                        eprintln!("error: unknown definition kind `{}`", kind);
                        return ExitCode::from(1);
                    }
                }
            } else {
                let t = type_ref
                    .map(|t| format!(" : {}", t))
                    .unwrap_or_default();
                format!("{} {}{};", kind, name, t)
            }
        }
    };

    // Determine where to insert
    let is_usage_like = !kind.contains("def") && !kind.contains("package")
        && !kind.contains("pkg") && kind != "requirement" && kind != "req"
        && kind != "import";
    let target_parent: Option<String> = if let Some(parent) = inside {
        Some(parent.to_string())
    } else if is_usage_like {
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
            None
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

    if dry_run {
        print!("{}", edit::diff(&source, &result, &path_str));
    } else {
        if let Err(e) = std::fs::write(file, &result) {
            eprintln!("error: cannot write `{}`: {}", path_str, e);
            return ExitCode::from(1);
        }
        eprintln!("Added `{}` to {}", name, path_str);

        // Suggest import if type reference is not defined in this file
        let refs_to_check: Vec<&str> = [type_ref, extends]
            .iter()
            .filter_map(|r| *r)
            .filter(|r| !r.contains("::"))  // Skip already-qualified refs
            .collect();
        if !refs_to_check.is_empty() {
            let updated = std::fs::read_to_string(file).unwrap_or_default();
            let updated_model = sysml_parser::parse_file(&path_str, &updated);
            for tr in refs_to_check {
                let defined = updated_model.definitions.iter().any(|d| d.name == tr);
                if !defined {
                    eprintln!("  hint: `{}` is not defined in this file. You may need:", tr);
                    eprintln!("    sysml add {} import '...::{}'", path_str, tr);
                }
            }
        }
    }
    ExitCode::SUCCESS
}
