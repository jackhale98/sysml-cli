use sysml_core::model::{simple_name, Model};
use tower_lsp::lsp_types::{DocumentHighlight, DocumentHighlightKind};

use crate::convert::span_to_range;

/// Find all highlights for `name` within a single model.
/// Returns the definition site as Write, and all reference sites as Read.
pub fn document_highlights(model: &Model, name: &str) -> Vec<DocumentHighlight> {
    let target = simple_name(name);
    let mut highlights = Vec::new();

    // Definition site
    for def in &model.definitions {
        if def.name == target {
            highlights.push(DocumentHighlight {
                range: span_to_range(&def.span),
                kind: Some(DocumentHighlightKind::WRITE),
            });
        }
        // Supertype reference
        if let Some(ref st) = def.super_type {
            if simple_name(st) == target {
                highlights.push(DocumentHighlight {
                    range: span_to_range(&def.span),
                    kind: Some(DocumentHighlightKind::READ),
                });
            }
        }
    }

    // Usage sites
    for usage in &model.usages {
        if usage.name == target {
            highlights.push(DocumentHighlight {
                range: span_to_range(&usage.span),
                kind: Some(DocumentHighlightKind::WRITE),
            });
        }
        if let Some(ref tr) = usage.type_ref {
            if simple_name(tr) == target {
                highlights.push(DocumentHighlight {
                    range: span_to_range(&usage.span),
                    kind: Some(DocumentHighlightKind::READ),
                });
            }
        }
    }

    // Type references
    for tr in &model.type_references {
        if simple_name(&tr.name) == target {
            highlights.push(DocumentHighlight {
                range: span_to_range(&tr.span),
                kind: Some(DocumentHighlightKind::READ),
            });
        }
    }

    highlights
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn highlights_definition_and_usage() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let hl = document_highlights(&model, "Engine");
        assert!(
            hl.len() >= 2,
            "should highlight def + usage, got {}",
            hl.len()
        );
        let writes: Vec<_> = hl
            .iter()
            .filter(|h| h.kind == Some(DocumentHighlightKind::WRITE))
            .collect();
        let reads: Vec<_> = hl
            .iter()
            .filter(|h| h.kind == Some(DocumentHighlightKind::READ))
            .collect();
        assert!(
            !writes.is_empty(),
            "should have write highlight for definition"
        );
        assert!(!reads.is_empty(), "should have read highlight for type ref");
    }

    #[test]
    fn highlights_supertype() {
        let source = "part def Base;\npart def Sub :> Base;\n";
        let model = parse_file("test.sysml", source);
        let hl = document_highlights(&model, "Base");
        assert!(hl.len() >= 2, "should highlight def + supertype ref");
    }

    #[test]
    fn no_highlights_for_unknown() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let hl = document_highlights(&model, "Unknown");
        assert!(hl.is_empty());
    }

    #[test]
    fn multiple_usages_highlighted() {
        let source =
            "part def Engine;\npart def Car {\n    part e1 : Engine;\n    part e2 : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let hl = document_highlights(&model, "Engine");
        assert!(
            hl.len() >= 3,
            "should have def + 2 usages, got {}",
            hl.len()
        );
    }

    #[test]
    fn usage_name_highlighted_as_write() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let hl = document_highlights(&model, "engine");
        let writes: Vec<_> = hl
            .iter()
            .filter(|h| h.kind == Some(DocumentHighlightKind::WRITE))
            .collect();
        assert!(
            !writes.is_empty(),
            "usage name should be highlighted as write"
        );
    }
}
