use sysml_core::model::{DefKind, Model};
use tower_lsp_server::ls_types::{DocumentSymbol, SymbolKind};

use crate::convert::span_to_range;

pub fn def_kind_to_symbol_kind(kind: DefKind) -> SymbolKind {
    match kind {
        DefKind::Package | DefKind::Namespace => SymbolKind::MODULE,
        DefKind::Part | DefKind::Item | DefKind::Class | DefKind::Struct | DefKind::Datatype => {
            SymbolKind::CLASS
        }
        DefKind::Port | DefKind::Interface | DefKind::Connector => SymbolKind::INTERFACE,
        DefKind::Action | DefKind::Calc | DefKind::Function | DefKind::Behavior | DefKind::Step => {
            SymbolKind::FUNCTION
        }
        DefKind::State => SymbolKind::ENUM,
        DefKind::Attribute | DefKind::Feature => SymbolKind::PROPERTY,
        DefKind::Requirement | DefKind::Concern | DefKind::Verification | DefKind::Analysis => {
            SymbolKind::EVENT
        }
        DefKind::Constraint | DefKind::Predicate => SymbolKind::OPERATOR,
        DefKind::Enum => SymbolKind::ENUM,
        DefKind::Connection | DefKind::Flow | DefKind::Allocation | DefKind::Assoc => {
            SymbolKind::STRUCT
        }
        DefKind::View | DefKind::Viewpoint | DefKind::Rendering => SymbolKind::MODULE,
        DefKind::UseCase | DefKind::Occurrence | DefKind::Interaction => SymbolKind::EVENT,
        DefKind::Metaclass | DefKind::Classifier | DefKind::Type => SymbolKind::CLASS,
        DefKind::Expr => SymbolKind::FUNCTION,
        DefKind::Metadata | DefKind::Annotation => SymbolKind::CONSTANT,
    }
}

fn usage_kind_to_symbol_kind(kind: &str) -> SymbolKind {
    match kind {
        "part" | "item" => SymbolKind::FIELD,
        "port" => SymbolKind::INTERFACE,
        "action" | "calc" => SymbolKind::FUNCTION,
        "state" => SymbolKind::ENUM_MEMBER,
        "attribute" => SymbolKind::PROPERTY,
        "ref" | "connection" | "flow" | "allocation" => SymbolKind::STRUCT,
        "requirement" => SymbolKind::EVENT,
        "constraint" => SymbolKind::OPERATOR,
        _ => SymbolKind::VARIABLE,
    }
}

/// Build a hierarchical list of document symbols from a model.
#[allow(deprecated)] // DocumentSymbol::children is deprecated in newer lsp_types but required
pub fn document_symbols(model: &Model, source: &str) -> Vec<DocumentSymbol> {
    // Build top-level definitions (no parent), then nest children
    let mut top_level: Vec<DocumentSymbol> = Vec::new();

    for def in &model.definitions {
        if def.parent_def.is_some() {
            continue; // handled as child
        }

        let children = build_children(model, source, &def.name);
        let range = span_to_range(&def.span, source);

        top_level.push(DocumentSymbol {
            name: def.name.clone(),
            detail: def.super_type.clone(),
            kind: def_kind_to_symbol_kind(def.kind),
            tags: None,
            deprecated: None,
            range,
            selection_range: range,
            children: if children.is_empty() {
                None
            } else {
                Some(children)
            },
        });
    }

    top_level
}

#[allow(deprecated)]
fn build_children(model: &Model, source: &str, parent_name: &str) -> Vec<DocumentSymbol> {
    let mut children = Vec::new();

    // Child definitions
    for def in &model.definitions {
        if def.parent_def.as_deref() != Some(parent_name) {
            continue;
        }
        let grandchildren = build_children(model, source, &def.name);
        let range = span_to_range(&def.span, source);
        children.push(DocumentSymbol {
            name: def.name.clone(),
            detail: def.super_type.clone(),
            kind: def_kind_to_symbol_kind(def.kind),
            tags: None,
            deprecated: None,
            range,
            selection_range: range,
            children: if grandchildren.is_empty() {
                None
            } else {
                Some(grandchildren)
            },
        });
    }

    // Child usages
    for usage in &model.usages {
        if usage.parent_def.as_deref() != Some(parent_name) {
            continue;
        }
        let range = span_to_range(&usage.span, source);
        children.push(DocumentSymbol {
            name: usage.name.clone(),
            detail: usage.type_ref.clone(),
            kind: usage_kind_to_symbol_kind(&usage.kind),
            tags: None,
            deprecated: None,
            range,
            selection_range: range,
            children: None,
        });
    }

    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn nested_package_part_usage() {
        let source =
            "package Vehicles {\n    part def Vehicle {\n        part engine : Engine;\n    }\n}\n";
        let model = parse_file("test.sysml", source);
        let symbols = document_symbols(&model, source);

        assert_eq!(symbols.len(), 1, "expected one top-level symbol");
        assert_eq!(symbols[0].name, "Vehicles");
        assert_eq!(symbols[0].kind, SymbolKind::MODULE);

        let vehicle_children = symbols[0].children.as_ref().unwrap();
        assert_eq!(vehicle_children.len(), 1);
        assert_eq!(vehicle_children[0].name, "Vehicle");
        assert_eq!(vehicle_children[0].kind, SymbolKind::CLASS);

        let engine_children = vehicle_children[0].children.as_ref().unwrap();
        assert_eq!(engine_children.len(), 1);
        assert_eq!(engine_children[0].name, "engine");
        assert_eq!(engine_children[0].detail.as_deref(), Some("Engine"));
        assert_eq!(engine_children[0].kind, SymbolKind::FIELD);
    }

    #[test]
    fn symbol_kind_mapping() {
        let source = "part def P;\nport def I;\naction def A;\nstate def S;\nattribute def At;\n";
        let model = parse_file("test.sysml", source);
        let symbols = document_symbols(&model, source);

        let kinds: Vec<_> = symbols.iter().map(|s| (&s.name, s.kind)).collect();
        assert!(kinds.contains(&(&"P".to_string(), SymbolKind::CLASS)));
        assert!(kinds.contains(&(&"I".to_string(), SymbolKind::INTERFACE)));
        assert!(kinds.contains(&(&"A".to_string(), SymbolKind::FUNCTION)));
        assert!(kinds.contains(&(&"S".to_string(), SymbolKind::ENUM)));
        assert!(kinds.contains(&(&"At".to_string(), SymbolKind::PROPERTY)));
    }

    #[test]
    fn symbol_ranges_match_spans() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let symbols = document_symbols(&model, source);
        assert_eq!(symbols.len(), 1);
        // Should be on line 0 (0-based)
        assert_eq!(symbols[0].range.start.line, 0);
    }

    #[test]
    fn empty_file_empty_symbols() {
        let model = parse_file("test.sysml", "");
        let symbols = document_symbols(&model, "");
        assert!(symbols.is_empty());
    }
}
