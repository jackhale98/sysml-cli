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
) -> ExitCode {
    // Reinterpret positionals: clap fills file/kind/name in order.
    // When --stdout is set and `file` looks like a kind (not a path), shift args.
    let (eff_file, eff_kind, eff_name) = if stdout || teach {
        // For stdout/teach mode: if file is set but kind is not, the user
        // wrote `add --stdout part-def Vehicle` and clap put "part-def" in file.
        match (file, kind, name) {
            (Some(f), Some(k), None) => {
                // file="part-def", kind="Vehicle", name=None
                // Shift: kind=file, name=kind
                (None, Some(f.to_string_lossy().to_string()), Some(k.to_string()))
            }
            (Some(f), None, None) => {
                // Only one positional — could be kind with missing name
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
                       members, exposes, filter, teach, type_ref)
        }
        // File but no kind/name → guided file mode
        (Some(file), None, None) if !stdout => {
            run_wizard_mode_with_file(file)
        }
        // File + kind + name → direct insert
        (Some(file), Some(kind), Some(name)) => {
            if stdout {
                run_stdout(kind, name, extends, is_abstract, short_name, doc,
                           members, exposes, filter, teach, type_ref)
            } else {
                run_insert(file, kind, name, type_ref, inside, dry_run,
                           doc, extends, is_abstract, short_name, members)
            }
        }
        // Partial args
        _ => {
            eprintln!("error: provide either no args (wizard), --stdout <kind> <name>, or <file> <kind> <name>");
            ExitCode::from(1)
        }
    }
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
) -> ExitCode {
    // For teach mode, delegate to scaffold
    if teach {
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

    // Check if this is a usage kind (no "def" in kind, not package)
    let is_def_kind = kind.contains("def") || kind.contains("package")
        || kind.contains("pkg");

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

        let parsed_members: Vec<template::MemberSpec> = members
            .iter()
            .filter_map(|s| template::parse_member_spec(s))
            .collect();

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

/// Concept-first interactive wizard for `sysml add` with no arguments.
fn run_wizard_mode() -> ExitCode {
    use sysml_core::interactive::*;
    use crate::wizard::CliWizardRunner;

    let runner = CliWizardRunner::new();
    if !runner.is_interactive() {
        eprintln!("error: interactive wizard requires a terminal");
        eprintln!("Usage: sysml add <file> <kind> <name>");
        return ExitCode::from(1);
    }

    // Step 1: What are you creating?
    let concept_step = WizardStep::choice(
        "concept",
        "What are you creating?",
        vec![
            ("part-def", "A new type: physical component or assembly"),
            ("port-def", "A new type: interaction point (port)"),
            ("action-def", "A new type: behavior or process"),
            ("requirement", "A new type: requirement or constraint"),
            ("state-def", "A new type: state machine"),
            ("attribute-def", "A new type: data type or property"),
            ("enum-def", "A new type: enumeration"),
            ("calc-def", "A new type: calculation or formula"),
            ("part", "An instance: component (part usage)"),
            ("port", "An instance: port (port usage)"),
            ("attribute", "An instance: property (attribute usage)"),
            ("action", "An instance: action step"),
            ("package", "A package (organizational grouping)"),
            ("other", "Something else (I know the SysML kind)"),
        ],
    ).with_explanation("SysML has definitions (reusable types) and usages (instances).");

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

    let is_def = kind.contains("def") || kind.contains("package") || kind.contains("pkg")
        || kind == "requirement";

    // Step 2: Name
    let name_step = WizardStep::string("name", "Name");
    let name = match runner.run_step(&name_step) {
        Some(WizardAnswer::String(s)) if !s.is_empty() => s,
        _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
    };

    // Step 3: Doc (optional)
    let doc_step = WizardStep::string("doc", "Brief description (Enter to skip)").optional();
    let doc = match runner.run_step(&doc_step) {
        Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    };

    // Step 4: For definitions — extends? For usages — type reference?
    let extends = if is_def {
        let ext_step = WizardStep::string("extends", "Extend another type? (Enter to skip)").optional();
        match runner.run_step(&ext_step) {
            Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
            _ => None,
        }
    } else {
        None
    };

    let type_ref = if !is_def {
        let type_step = WizardStep::string("type_ref", "Type reference? (Enter to skip)")
            .with_explanation("The definition this usage instantiates.");
        match runner.run_step(&type_step.optional()) {
            Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
            _ => None,
        }
    } else {
        None
    };

    // Step 5: Generate SysML text
    let sysml_text = if is_def {
        if let Some(def_kind) = template::parse_template_kind(&kind) {
            let opts = template::TemplateOptions {
                kind: def_kind,
                name: name.clone(),
                super_type: extends.clone(),
                is_abstract: false,
                short_name: None,
                doc: doc.clone(),
                members: Vec::new(),
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

    // Step 6: Where to put it?
    let dest_step = WizardStep::choice(
        "destination",
        "Where should it go?",
        vec![
            ("stdout", "Print to terminal"),
            ("file", "Add to existing file"),
        ],
    );
    let dest = match runner.run_step(&dest_step) {
        Some(WizardAnswer::String(s)) => s,
        _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
    };

    if dest == "stdout" {
        println!("{}", sysml_text);
        return ExitCode::SUCCESS;
    }

    // Select target file
    let target = match crate::model_writer::select_target_file(std::path::Path::new(".")) {
        Some(p) => p,
        None => {
            eprintln!("No file selected. Printing to stdout:");
            println!("{}", sysml_text);
            return ExitCode::SUCCESS;
        }
    };

    // For usages, ask which definition to insert inside
    let inside = if !is_def {
        crate::model_writer::select_parent_def(&target)
    } else {
        None
    };

    match crate::model_writer::write_to_model(&target, &sysml_text, inside.as_deref()) {
        Ok(()) => {
            eprintln!("Wrote {} to {}", name, target.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// Guided file mode: `sysml add <file>` with no kind/name.
fn run_wizard_mode_with_file(file: &PathBuf) -> ExitCode {
    use sysml_core::interactive::*;
    use crate::wizard::CliWizardRunner;

    let runner = CliWizardRunner::new();
    if !runner.is_interactive() {
        eprintln!("error: guided mode requires an interactive terminal");
        eprintln!("Usage: sysml add <file> <kind> <name>");
        return ExitCode::from(1);
    }

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let model = sysml_parser::parse_file(&path_str, &source);

    // Show what's in the file
    let def_names: Vec<&str> = model.definitions.iter().map(|d| d.name.as_str()).collect();
    if !def_names.is_empty() {
        eprintln!("File contains: {}", def_names.join(", "));
    }

    // Ask what kind to add
    let concept_step = WizardStep::choice(
        "concept",
        "What do you want to add?",
        vec![
            ("part-def", "New definition (part, port, action, etc.)"),
            ("part", "New usage inside a definition"),
        ],
    );
    let is_def = match runner.run_step(&concept_step) {
        Some(WizardAnswer::String(s)) => s.contains("def"),
        _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
    };

    if is_def {
        // For definitions, ask kind and name then insert at top level
        let kind_step = WizardStep::choice("kind", "Definition kind", vec![
            ("part-def", "part def"),
            ("port-def", "port def"),
            ("action-def", "action def"),
            ("requirement", "requirement def"),
            ("state-def", "state def"),
            ("attribute-def", "attribute def"),
            ("enum-def", "enum def"),
            ("calc-def", "calc def"),
        ]);
        let kind = match runner.run_step(&kind_step) {
            Some(WizardAnswer::String(s)) => s,
            _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
        };

        let name_step = WizardStep::string("name", "Name");
        let name = match runner.run_step(&name_step) {
            Some(WizardAnswer::String(s)) if !s.is_empty() => s,
            _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
        };

        let doc_step = WizardStep::string("doc", "Description (Enter to skip)").optional();
        let doc = match runner.run_step(&doc_step) {
            Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
            _ => None,
        };

        let def_kind = match template::parse_template_kind(&kind) {
            Some(k) => k,
            None => { eprintln!("error: unknown kind `{}`", kind); return ExitCode::FAILURE; }
        };

        let sysml_text = template::generate_template(&template::TemplateOptions {
            kind: def_kind,
            name: name.clone(),
            super_type: None,
            is_abstract: false,
            short_name: None,
            doc,
            members: Vec::new(),
            exposes: Vec::new(),
            filter: None,
            indent: 0,
        });

        eprintln!("\nPreview:");
        for line in sysml_text.lines() {
            eprintln!("  {}", line);
        }
        eprintln!();

        match crate::model_writer::write_to_model(file, &sysml_text, None) {
            Ok(()) => {
                eprintln!("Added `{}` to {}", name, path_str);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::FAILURE
            }
        }
    } else {
        // For usages, ask kind, name, type, and parent
        let kind_step = WizardStep::choice("kind", "Usage kind", vec![
            ("part", "part"),
            ("port", "port"),
            ("attribute", "attribute"),
            ("action", "action"),
            ("state", "state"),
            ("item", "item"),
        ]);
        let kind = match runner.run_step(&kind_step) {
            Some(WizardAnswer::String(s)) => s,
            _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
        };

        let name_step = WizardStep::string("name", "Name");
        let name = match runner.run_step(&name_step) {
            Some(WizardAnswer::String(s)) if !s.is_empty() => s,
            _ => { eprintln!("Cancelled."); return ExitCode::FAILURE; }
        };

        let type_step = WizardStep::string("type_ref", "Type reference (Enter to skip)").optional();
        let type_ref = match runner.run_step(&type_step) {
            Some(WizardAnswer::String(s)) if !s.is_empty() => Some(s),
            _ => None,
        };

        let t = type_ref.as_deref()
            .map(|t| format!(" : {}", t))
            .unwrap_or_default();
        let sysml_text = format!("{} {}{};", kind, name, t);

        // Ask which definition to insert inside
        let inside = crate::model_writer::select_parent_def(file);

        eprintln!("\nPreview:");
        eprintln!("  {}", sysml_text);
        eprintln!();

        match crate::model_writer::write_to_model(file, &sysml_text, inside.as_deref()) {
            Ok(()) => {
                eprintln!("Added `{}` to {}", name, path_str);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {}", e);
                ExitCode::FAILURE
            }
        }
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
) -> ExitCode {
    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let model = sysml_parser::parse_file(&path_str, &source);

    // Only generate a full definition template for explicit def kinds
    let is_def_kind = kind.contains("def") || kind.contains("package")
        || kind.contains("pkg");
    let text = if is_def_kind {
        if let Some(def_kind) = template::parse_template_kind(kind) {
            let super_type = extends.or(type_ref).map(|s| s.to_string());
            let parsed_members: Vec<template::MemberSpec> = members
                .iter()
                .filter_map(|s| template::parse_member_spec(s))
                .collect();
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
            .map(|t| format!(" : {}", t))
            .unwrap_or_default();
        format!("{} {}{};", kind, name, t)
    };

    // Determine where to insert
    let target_parent: Option<String> = if let Some(parent) = inside {
        Some(parent.to_string())
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
    }
    ExitCode::SUCCESS
}
