//! View command: render model-defined views (`view def` +
//! `@TableRendering`) as tables. The rendering semantics live entirely in
//! `sysml_core::view_render`; this file is terminal formatting only.

use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::view_render::{available_views, render_view};

use crate::Cli;

pub fn run(cli: &Cli, name: Option<&str>, files: &[PathBuf], renderer: &str) -> ExitCode {
    // Positionals are sorted by what they are, not by where they sit:
    // anything that names an existing path is a file, the one that does
    // not is the view name. So `sysml view Fmea m.sysml` and
    // `sysml view m.sysml Fmea` both work, and `sysml view m.sysml`
    // alone lists the views. Every other command takes files first;
    // this keeps that habit from being an error here.
    let mut positional: Vec<String> = Vec::new();
    if let Some(n) = name {
        positional.push(n.to_string());
    }
    positional.extend(files.iter().map(|f| f.to_string_lossy().to_string()));
    let (paths, names): (Vec<String>, Vec<String>) = positional
        .into_iter()
        .partition(|p| std::path::Path::new(p).exists());
    if names.len() > 1 {
        eprintln!(
            "error: expected one view name, got {}: {}",
            names.len(),
            names.join(", ")
        );
        eprintln!("note: names that are not existing files are read as view names");
        return ExitCode::FAILURE;
    }
    let name = names.first().map(|s| s.as_str());
    let files: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let files = &files[..];

    let Some(models) = crate::load_per_file_models(cli, files) else {
        return ExitCode::FAILURE;
    };

    // Rows come from the files the user named; include paths are
    // resolution context (and where library view defs live), not model
    // content. Without this, `ModelStats` counted every definition in
    // the libraries and `trace`-style views listed their requirements.
    let (resolved, primary) = crate::helpers::resolve_files(cli, files);
    let targets: Vec<String> = resolved
        .iter()
        .take(primary)
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    let Some(name) = name else {
        // List available views.
        let views = available_views(&models);
        if views.is_empty() {
            eprintln!("no view defs found (are the libraries on the include path?)");
            return ExitCode::FAILURE;
        }
        if cli.format == "json" {
            let items: Vec<_> = views
                .iter()
                .map(|(n, f, spec)| {
                    serde_json::json!({ "name": n, "file": f, "renderable": spec })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&items).unwrap());
        } else {
            for (n, f, spec) in &views {
                let label = if *spec {
                    " ".to_string()
                } else if let Some(r) = models
                    .iter()
                    .flat_map(|m| &m.views)
                    .find(|v| &v.name == n)
                    .and_then(|v| v.render_as.as_deref())
                {
                    format!("(diagram: {r})")
                } else {
                    "(no rendering declared)".to_string()
                };
                println!("{:<28} {} {}", n, label, f);
            }
        }
        return ExitCode::SUCCESS;
    };

    let table = match render_view(&models, &targets, name) {
        Ok(t) => t,
        Err(e) => {
            // A view def with a `render as` clause is a diagram view:
            // route through the shared diagram machinery, with the
            // view's expose/filter clauses selecting content.
            if let Some(kind) = models
                .iter()
                .flat_map(|m| &m.views)
                .find(|v| v.name == name)
                .and_then(|v| v.render_as.as_deref())
                .and_then(sysml_core::diagram::DiagramKind::from_render_clause)
            {
                return render_diagram_view(name, files, &models, kind, renderer);
            }
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    for w in &table.warnings {
        eprintln!("warning: {w}");
    }

    crate::output::print_table(&cli.format, &table);
    ExitCode::SUCCESS
}

/// Render a `render as` view def as a diagram: merge the loaded models,
/// take the diagram kind from the render clause, and apply the view's
/// expose/filter selection. State/action diagram flavors re-extract from
/// the first primary file's source.
fn render_diagram_view(
    name: &str,
    files: &[std::path::PathBuf],
    models: &[sysml_core::model::Model],
    kind: sysml_core::diagram::DiagramKind,
    renderer: &str,
) -> ExitCode {
    let Some(format) = sysml_core::diagram::DiagramFormat::from_str(renderer) else {
        eprintln!(
            "error: unknown renderer `{renderer}`. Available: mermaid, plantuml (puml), dot, d2"
        );
        return ExitCode::FAILURE;
    };
    let mut merged = sysml_core::model::Model::new("<view>".to_string());
    for m in models {
        merged.merge(m.clone());
    }
    let (path_str, source) = match files.first() {
        Some(f) => match crate::read_source(f) {
            Ok(v) => v,
            Err(code) => return code,
        },
        None => (String::new(), String::new()),
    };
    super::diagram::build_and_print(
        &merged,
        &path_str,
        &source,
        kind,
        format,
        None,
        Some(name),
        sysml_core::diagram::LayoutDirection::default(),
        None,
    )
}
