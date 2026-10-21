use sysml_core::model::{simple_name, Model, Span};

pub use sysml_core::lang::definition::find_identifier_at_offset;
#[cfg(test)]
use sysml_core::lang::definition::goto_definition_in_file;

use crate::state::WorldState;

/// Resolve a name: try in-file first, then workspace defs. Returns (uri, span).
pub fn goto_definition(
    model: &Model,
    name: &str,
    current_uri: &str,
    state: &WorldState,
) -> Option<(String, Span)> {
    let simple = simple_name(name);

    // Try same-file first
    if let Some(def) = model.find_def(simple) {
        return Some((current_uri.to_string(), def.span.clone()));
    }

    // Try workspace defs
    if let Some(loc) = state.workspace_defs.get(simple) {
        return Some((loc.uri.clone(), loc.span.clone()));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn find_type_ref_at_cursor() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        // Find the byte offset of "Engine" in the type reference on line 3
        let engine_ref_pos = source.rfind("Engine").unwrap();
        let name = find_identifier_at_offset(&model, source, engine_ref_pos);
        assert_eq!(name.as_deref(), Some("Engine"));
    }

    #[test]
    fn goto_def_resolves_type_ref() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let span = goto_definition_in_file(&model, "Engine");
        assert!(span.is_some());
        let span = span.unwrap();
        // Engine def is on line 1 (1-based)
        assert_eq!(span.start_row, 1);
    }

    #[test]
    fn cursor_on_definition_name_still_resolves() {
        // Clicking on a definition name should still find the definition
        let source = "part def Engine;\n";
        let model = parse_file("test.sysml", source);
        let span = goto_definition_in_file(&model, "Engine");
        assert!(span.is_some());
    }

    #[test]
    fn unknown_type_returns_none() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let span = goto_definition_in_file(&model, "Unknown");
        assert!(span.is_none());
    }

    #[test]
    fn supertype_navigation() {
        let source = "part def Base;\npart def Sub :> Base;\n";
        let model = parse_file("test.sysml", source);
        // Find "Base" in the supertype position
        let base_pos = source.rfind("Base").unwrap();
        let name = find_identifier_at_offset(&model, source, base_pos);
        assert_eq!(name.as_deref(), Some("Base"));
        // Resolve it
        let span = goto_definition_in_file(&model, "Base");
        assert!(span.is_some());
        assert_eq!(span.unwrap().start_row, 1);
    }

    #[test]
    fn cross_file_resolution() {
        let source_a = "part def Engine;\n";
        let source_b = "part def Vehicle {\n    part engine : Engine;\n}\n";
        let model_a = parse_file("a.sysml", source_a);
        let model_b = parse_file("b.sysml", source_b);

        let state = WorldState::new();
        state.index_model_defs("file:///a.sysml", &model_a);
        state.index_model_defs("file:///b.sysml", &model_b);

        // From b.sysml, resolve Engine -> should find it in a.sysml
        let result = goto_definition(&model_b, "Engine", "file:///b.sysml", &state);
        assert!(result.is_some());
        let (uri, _span) = result.unwrap();
        assert_eq!(uri, "file:///a.sysml");
    }
}
