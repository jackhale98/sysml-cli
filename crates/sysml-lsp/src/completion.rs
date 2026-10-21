//! Completion: context detection and items come from
//! `sysml_core::lang::completion`; this module maps them to LSP items.

use sysml_core::lang::completion::{self as lang, ItemOrigin};
use sysml_core::model::{DefKind, Model};
use tower_lsp_server::ls_types::{CompletionItem, CompletionItemKind, Documentation};

pub use sysml_core::lang::completion::{detect_context, CompletionFilter};

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

fn usage_kind_to_completion_kind(kind: &str) -> CompletionItemKind {
    match kind {
        "part" | "item" => CompletionItemKind::FIELD,
        "port" => CompletionItemKind::INTERFACE,
        "attribute" | "feature" => CompletionItemKind::PROPERTY,
        "action" | "calc" => CompletionItemKind::FUNCTION,
        _ => CompletionItemKind::VARIABLE,
    }
}

fn to_lsp(item: lang::CompletionItem) -> CompletionItem {
    CompletionItem {
        kind: Some(match &item.origin {
            ItemOrigin::Def(kind) => def_kind_to_completion_kind(*kind),
            ItemOrigin::Member(kind) => usage_kind_to_completion_kind(kind),
            ItemOrigin::Inherited => CompletionItemKind::PROPERTY,
            ItemOrigin::Other => CompletionItemKind::VALUE,
        }),
        label: item.label,
        detail: item.detail,
        documentation: item.documentation.map(Documentation::String),
        sort_text: item.sort_text,
        ..Default::default()
    }
}

/// Completion items from the current file, workspace defs, and stdlib.
/// `filter` restricts which items are returned based on cursor context.
pub fn completions(
    model: &Model,
    workspace_defs: &[DefLocation],
    filter: &CompletionFilter,
) -> Vec<CompletionItem> {
    lang::completions(model, workspace_defs, filter)
        .into_iter()
        .map(to_lsp)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn completions_include_file_defs() {
        let source = "part def Vehicle;\npart def Engine;\n";
        let model = parse_file("test.sysml", source);
        let items = completions(&model, &[], &CompletionFilter::All);
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"Vehicle"));
        assert!(names.contains(&"Engine"));
    }

    #[test]
    fn completions_kind_mapping() {
        let source = "part def Vehicle;\nport def PowerPort;\naction def Drive;\n";
        let model = parse_file("test.sysml", source);
        let items = completions(&model, &[], &CompletionFilter::All);

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
        let items = completions(&model, &[], &CompletionFilter::All);
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"ScalarValues"),
            "stdlib should provide ScalarValues, got: {:?}",
            names.iter().take(20).collect::<Vec<_>>()
        );
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
        let items = completions(&model, &workspace, &CompletionFilter::All);
        let vehicle_count = items.iter().filter(|i| i.label == "Vehicle").count();
        assert_eq!(vehicle_count, 1, "should deduplicate");
    }

    // --- Context detection ---

    #[test]
    fn detect_type_position() {
        let source = "part engine : ";
        let ctx = detect_context(source, source.len());
        assert_eq!(ctx, CompletionFilter::TypePosition);
    }

    #[test]
    fn detect_specialization() {
        let source = "part def Sub :> ";
        let ctx = detect_context(source, source.len());
        assert_eq!(ctx, CompletionFilter::Specialization);
    }

    #[test]
    fn detect_member_access() {
        let source = "vehicle.";
        let ctx = detect_context(source, source.len());
        assert!(matches!(ctx, CompletionFilter::MemberAccess(ref n) if n == "vehicle"));
    }

    #[test]
    fn detect_normal_context() {
        let source = "part def ";
        let ctx = detect_context(source, source.len());
        assert_eq!(ctx, CompletionFilter::All);
    }

    #[test]
    fn type_position_excludes_packages() {
        let source = "package Pkg { part def Vehicle; }\n";
        let model = parse_file("test.sysml", source);
        let all = completions(&model, &[], &CompletionFilter::All);
        let typed = completions(&model, &[], &CompletionFilter::TypePosition);
        // All includes Pkg, typed does not
        assert!(all.iter().any(|i| i.label == "Pkg"));
        assert!(!typed.iter().any(|i| i.label == "Pkg"));
    }

    #[test]
    fn member_access_shows_children() {
        let source = "part def Vehicle { part engine : Engine; attribute mass : Real; }\n";
        let model = parse_file("test.sysml", source);
        let items = completions(
            &model,
            &[],
            &CompletionFilter::MemberAccess("Vehicle".to_string()),
        );
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"engine"),
            "should include part member: {:?}",
            names
        );
        assert!(
            names.contains(&"mass"),
            "should include attribute member: {:?}",
            names
        );
    }

    #[test]
    fn detect_qualified_access() {
        let source = "attribute m :> ISQ::";
        let ctx = detect_context(source, source.len());
        assert!(matches!(ctx, CompletionFilter::QualifiedAccess(ref p) if p == "ISQ"));
    }

    #[test]
    fn qualified_access_lists_stdlib_members() {
        if sysml_core::stdlib::stdlib_files().is_empty() {
            return;
        }
        let model = parse_file("test.sysml", "part def X;\n");
        let items = completions(
            &model,
            &[],
            &CompletionFilter::QualifiedAccess("ISQ".to_string()),
        );
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"mass"),
            "ISQ:: must offer mass, got {} items: {:?}",
            names.len(),
            names.iter().take(15).collect::<Vec<_>>()
        );
    }

    #[test]
    fn type_position_includes_stdlib_ranked_below() {
        if sysml_core::stdlib::stdlib_files().is_empty() {
            return;
        }
        let model = parse_file("test.sysml", "part def Vehicle;\n");
        let items = completions(&model, &[], &CompletionFilter::TypePosition);
        let real = items.iter().find(|i| i.label == "Real");
        assert!(real.is_some(), "type position must offer stdlib Real");
        let vehicle = items.iter().find(|i| i.label == "Vehicle").unwrap();
        assert!(
            vehicle.sort_text.as_deref() < real.unwrap().sort_text.as_deref(),
            "workspace types rank above stdlib"
        );
    }

    #[test]
    fn member_access_through_usage_name() {
        let source =
            "part def Drone { attribute mass : Real; }\npart def Fleet { part drone : Drone; }\n";
        let model = parse_file("test.sysml", source);
        let items = completions(
            &model,
            &[],
            &CompletionFilter::MemberAccess("drone".to_string()),
        );
        let names: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"mass"),
            "usage-name member access must resolve through the type: {:?}",
            names
        );
    }
}
