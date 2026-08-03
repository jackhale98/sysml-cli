use sysml_core::model::Model;
use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position};

use crate::convert::span_to_range;

/// Generate inlay hints for a model — show inferred types on untyped usages
/// and multiplicity annotations.
pub fn inlay_hints(model: &Model, source: &str) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    for usage in &model.usages {
        // Show multiplicity hint for usages with multiplicity
        if let Some(ref mult) = usage.multiplicity {
            let range = span_to_range(&usage.span, source);
            hints.push(InlayHint {
                position: range.end,
                label: InlayHintLabel::String(format!(" {}", mult)),
                kind: Some(InlayHintKind::PARAMETER),
                text_edits: None,
                tooltip: None,
                padding_left: Some(true),
                padding_right: None,
                data: None,
            });
        }

        // Show type hint for usages without explicit type that have a same-named definition
        if usage.type_ref.is_none() && !usage.name.is_empty() {
            // Look for a definition with a name matching (capitalized) the usage name
            let capitalized = capitalize(&usage.name);
            if model.find_def(&capitalized).is_some() {
                let range = span_to_range(&usage.span, source);
                hints.push(InlayHint {
                    position: Position::new(range.start.line, range.end.character),
                    label: InlayHintLabel::String(format!(": {}", capitalized)),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(true),
                    padding_right: None,
                    data: None,
                });
            }
        }
    }

    hints
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn multiplicity_hints() {
        let source = "part def Vehicle { part wheels : Wheel [4]; }\n";
        let model = parse_file("test.sysml", source);
        let hints = inlay_hints(&model, source);
        let mult_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::PARAMETER))
            .collect();
        assert!(!mult_hints.is_empty(), "should have multiplicity hint");
    }

    #[test]
    fn type_inference_hint() {
        let source = "part def Engine;\npart def Vehicle { part engine; }\n";
        let model = parse_file("test.sysml", source);
        let hints = inlay_hints(&model, source);
        let type_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::TYPE))
            .collect();
        assert!(
            !type_hints.is_empty(),
            "should suggest Engine type for engine usage"
        );
    }

    #[test]
    fn no_hint_when_typed() {
        let source = "part def Engine;\npart def Vehicle { part engine : Engine; }\n";
        let model = parse_file("test.sysml", source);
        let hints = inlay_hints(&model, source);
        let type_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == Some(InlayHintKind::TYPE))
            .collect();
        assert!(type_hints.is_empty(), "typed usages should not get hints");
    }
}
