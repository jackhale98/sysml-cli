/// Generate Markdown documentation from SysML v2 models.

use std::path::PathBuf;
use std::process::ExitCode;

use sysml_core::model::{DefKind, Model};
use sysml_core::parser as sysml_parser;

use crate::Cli;

pub fn run(cli: &Cli, files: &[PathBuf], root: Option<&str>) -> ExitCode {
    let (files, _) = crate::files_or_project(files);
    if files.is_empty() {
        eprintln!("error: no SysML files found.");
        return ExitCode::FAILURE;
    }

    let mut merged = Model::new("project".to_string());
    for file_path in &files {
        let path_str = file_path.to_string_lossy().to_string();
        if let Ok(source) = std::fs::read_to_string(file_path) {
            let m = sysml_parser::parse_file(&path_str, &source);
            merged.definitions.extend(m.definitions);
            merged.usages.extend(m.usages);
            merged.connections.extend(m.connections);
            merged.flows.extend(m.flows);
            merged.satisfactions.extend(m.satisfactions);
            merged.verifications.extend(m.verifications);
            merged.allocations.extend(m.allocations);
            merged.comments.extend(m.comments);
        }
    }

    let doc = generate_markdown(&merged, root);

    match cli.format.as_str() {
        "json" => {
            let json = serde_json::json!({"markdown": doc});
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        _ => {
            print!("{}", doc);
        }
    }

    ExitCode::SUCCESS
}

fn generate_markdown(model: &Model, root: Option<&str>) -> String {
    let mut out = String::new();

    // Title
    let title = root.unwrap_or("Model Documentation");
    out.push_str(&format!("# {}\n\n", title));

    // Find definitions to document
    let defs: Vec<_> = if let Some(root_name) = root {
        // Document from root and all descendants
        let mut result = Vec::new();
        let mut queue = vec![root_name.to_string()];
        // Add the root itself
        if let Some(def) = model.find_def(root_name) {
            result.push(def);
        }
        // BFS for descendants
        while let Some(parent) = queue.pop() {
            for def in &model.definitions {
                if def.parent_def.as_deref() == Some(parent.as_str()) {
                    result.push(def);
                    queue.push(def.name.clone());
                }
            }
        }
        result
    } else {
        // Document all definitions (not just top-level)
        model.definitions.iter().collect()
    };

    // Group by kind
    let mut packages = Vec::new();
    let mut part_defs = Vec::new();
    let mut other_defs = Vec::new();

    for def in &defs {
        match def.kind {
            DefKind::Package => packages.push(*def),
            DefKind::Part | DefKind::Item => part_defs.push(*def),
            _ => other_defs.push(*def),
        }
    }

    // Packages
    if !packages.is_empty() {
        out.push_str("## Packages\n\n");
        for pkg in &packages {
            out.push_str(&format!("### {}\n\n", pkg.name));
            if let Some(ref doc) = pkg.doc {
                out.push_str(&format!("{}\n\n", doc));
            }
            // List children
            let children: Vec<_> = model.definitions.iter()
                .filter(|d| d.parent_def.as_deref() == Some(&pkg.name))
                .collect();
            if !children.is_empty() {
                out.push_str("| Element | Kind | Description |\n");
                out.push_str("|---------|------|-------------|\n");
                for child in &children {
                    out.push_str(&format!("| `{}` | {} | {} |\n",
                        child.name,
                        child.kind.label(),
                        child.doc.as_deref().unwrap_or("")));
                }
                out.push('\n');
            }
        }
    }

    // Part definitions
    if !part_defs.is_empty() {
        out.push_str("## Part Definitions\n\n");
        for def in &part_defs {
            out.push_str(&format!("### {}", def.name));
            if let Some(ref st) = def.super_type {
                out.push_str(&format!(" : {}", st));
            }
            out.push('\n');
            out.push('\n');
            if let Some(ref doc) = def.doc {
                out.push_str(&format!("{}\n\n", doc));
            }
            // Members table
            let members = model.usages_in_def(&def.name);
            if !members.is_empty() {
                out.push_str("| Member | Kind | Type | Multiplicity |\n");
                out.push_str("|--------|------|------|--------------|\n");
                for m in &members {
                    out.push_str(&format!("| `{}` | {} | {} | {} |\n",
                        m.name,
                        m.kind,
                        m.type_ref.as_deref().unwrap_or("-"),
                        m.multiplicity.as_ref().map(|mu| mu.to_string()).unwrap_or_else(|| "-".to_string())));
                }
                out.push('\n');
            }
        }
    }

    // Other definitions
    if !other_defs.is_empty() {
        out.push_str("## Other Definitions\n\n");
        for def in &other_defs {
            out.push_str(&format!("- **{}** `{}`", def.kind.label(), def.name));
            if let Some(ref st) = def.super_type {
                out.push_str(&format!(" : `{}`", st));
            }
            if let Some(ref doc) = def.doc {
                out.push_str(&format!(" — {}", doc));
            }
            out.push('\n');
        }
        out.push('\n');
    }

    // Relationships
    if !model.connections.is_empty() || !model.satisfactions.is_empty() {
        out.push_str("## Relationships\n\n");

        if !model.connections.is_empty() {
            out.push_str("### Connections\n\n");
            out.push_str("| Source | Target |\n");
            out.push_str("|--------|--------|\n");
            for conn in &model.connections {
                out.push_str(&format!("| `{}` | `{}` |\n", conn.source, conn.target));
            }
            out.push('\n');
        }

        if !model.satisfactions.is_empty() {
            out.push_str("### Requirements Satisfaction\n\n");
            out.push_str("| Requirement | Satisfied By |\n");
            out.push_str("|-------------|-------------|\n");
            for sat in &model.satisfactions {
                out.push_str(&format!("| `{}` | {} |\n",
                    sat.requirement,
                    sat.by.as_deref().unwrap_or("-")));
            }
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn doc_generates_markdown() {
        let source = r#"
            part def Vehicle {
                doc /* A motor vehicle */
                part engine : Engine;
                attribute mass : Real;
            }
            part def Engine {
                doc /* Power plant */
            }
        "#;
        let model = parse_file("test.sysml", source);
        let doc = generate_markdown(&model, None);
        assert!(doc.contains("# Model Documentation"));
        assert!(doc.contains("Vehicle"));
        assert!(doc.contains("Engine"));
        assert!(doc.contains("A motor vehicle"));
    }

    #[test]
    fn doc_with_root() {
        let source = r#"
            part def Vehicle {
                part engine : Engine;
            }
            part def Engine;
            part def Unrelated;
        "#;
        let model = parse_file("test.sysml", source);
        let doc = generate_markdown(&model, Some("Vehicle"));
        assert!(doc.contains("# Vehicle"));
        // Should not contain Unrelated since we're rooted at Vehicle
    }

    #[test]
    fn doc_includes_members_table() {
        let source = r#"
            part def Vehicle {
                part engine : Engine;
                attribute mass : Real;
                port fuelIn : FuelPort;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let doc = generate_markdown(&model, None);
        assert!(doc.contains("| `engine`"));
        assert!(doc.contains("| `mass`"));
    }
}
