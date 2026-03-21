use sysml_core::model::{DefKind, Model};
use sysml_core::stdlib;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use std::collections::HashSet;

use crate::state::DefLocation;

fn def_kind_to_completion_kind(kind: DefKind) -> CompletionItemKind {
    match kind {
        DefKind::Package | DefKind::Namespace => CompletionItemKind::MODULE,
        DefKind::Part | DefKind::Item | DefKind::Class | DefKind::Struct | DefKind::Datatype => {
            CompletionItemKind::CLASS
        }
        DefKind::Port | DefKind::Interface | DefKind::Connector => CompletionItemKind::INTERFACE,
        DefKind::Action | DefKind::Calc | DefKind::Function | DefKind::Behavior | DefKind::Step => {
            CompletionItemKind::FUNCTION
        }
        DefKind::State | DefKind::Enum => CompletionItemKind::ENUM,
        DefKind::Attribute | DefKind::Feature => CompletionItemKind::PROPERTY,
        DefKind::Requirement | DefKind::Concern | DefKind::Verification | DefKind::Analysis => {
            CompletionItemKind::EVENT
        }
        DefKind::Constraint | DefKind::Predicate => CompletionItemKind::OPERATOR,
        _ => CompletionItemKind::VALUE,
    }
}

/// Generate completion items from the current file, workspace defs, and stdlib.
pub fn completions(
    model: &Model,
    workspace_defs: &[DefLocation],
) -> Vec<CompletionItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    // Current file definitions
    for def in &model.definitions {
        if seen.insert(def.name.clone()) {
            items.push(CompletionItem {
                label: def.name.clone(),
                kind: Some(def_kind_to_completion_kind(def.kind)),
                detail: Some(def.kind.label().to_string()),
                documentation: def.doc.as_ref().map(|d| {
                    tower_lsp::lsp_types::Documentation::String(d.clone())
                }),
                ..Default::default()
            });
        }
    }

    // Workspace definitions (cross-file)
    for loc in workspace_defs {
        if seen.insert(loc.name.clone()) {
            items.push(CompletionItem {
                label: loc.name.clone(),
                kind: Some(def_kind_to_completion_kind(loc.kind)),
                detail: Some(loc.kind.label().to_string()),
                documentation: loc.doc.as_ref().map(|d| {
                    tower_lsp::lsp_types::Documentation::String(d.clone())
                }),
                ..Default::default()
            });
        }
    }

    // Standard library definitions
    for stdlib_model in stdlib::parse_stdlib() {
        for def in &stdlib_model.definitions {
            if seen.insert(def.name.clone()) {
                items.push(CompletionItem {
                    label: def.name.clone(),
                    kind: Some(def_kind_to_completion_kind(def.kind)),
                    detail: Some(format!("{} (stdlib)", def.kind.label())),
                    documentation: def.doc.as_ref().map(|d| {
                        tower_lsp::lsp_types::Documentation::String(d.clone())
                    }),
                    ..Default::default()
                });
            }
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn completions_include_file_defs() {
        let source = "part def Vehicle;\npart def Engine;\n";
        let model = parse_file("test.sysml", source);
        let items = completions(&model, &[]);
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"Vehicle"));
        assert!(names.contains(&"Engine"));
    }

    #[test]
    fn completions_kind_mapping() {
        let source = "part def Vehicle;\nport def PowerPort;\naction def Drive;\n";
        let model = parse_file("test.sysml", source);
        let items = completions(&model, &[]);

        let vehicle = items.iter().find(|i| i.label == "Vehicle").unwrap();
        assert_eq!(vehicle.kind, Some(CompletionItemKind::CLASS));

        let port = items.iter().find(|i| i.label == "PowerPort").unwrap();
        assert_eq!(port.kind, Some(CompletionItemKind::INTERFACE));

        let action = items.iter().find(|i| i.label == "Drive").unwrap();
        assert_eq!(action.kind, Some(CompletionItemKind::FUNCTION));
    }

    #[test]
    fn completions_include_stdlib() {
        if sysml_core::stdlib::stdlib_files().is_empty() {
            eprintln!("SKIP: stdlib not embedded");
            return;
        }
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let items = completions(&model, &[]);
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"ScalarValues"), "stdlib should provide ScalarValues, got: {:?}", names.iter().take(20).collect::<Vec<_>>());
    }

    #[test]
    fn completions_dedup() {
        // If a name exists in both file and workspace, only one entry
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let workspace = vec![DefLocation {
            uri: "file:///other.sysml".to_string(),
            name: "Vehicle".to_string(),
            kind: DefKind::Part,
            span: sysml_core::model::Span::default(),
            doc: None,
            super_type: None,
            qualified_name: None,
        }];
        let items = completions(&model, &workspace);
        let vehicle_count = items.iter().filter(|i| i.label == "Vehicle").count();
        assert_eq!(vehicle_count, 1, "should deduplicate");
    }
}
