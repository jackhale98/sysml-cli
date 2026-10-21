//! Rename: occurrences come from `sysml_core::lang::rename`; this module
//! builds the LSP workspace edit.

use std::collections::HashMap;

use sysml_core::lang::rename as lang;
use sysml_core::model::Model;
use tower_lsp_server::ls_types::{Range, TextEdit, Uri, WorkspaceEdit};

use crate::convert::offset_to_position;

/// Build a WorkspaceEdit that renames `old_name` to `new_name` across all provided models.
pub fn rename_symbol(
    models: &[(&str, &str, &Model)], // (uri, source, model)
    old_name: &str,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();
    for (uri_str, source, occs) in lang::rename_occurrences(models, old_name) {
        let uri = uri_str.parse::<Uri>().ok()?;
        let edits = occs
            .iter()
            .map(|occ| TextEdit {
                range: Range::new(
                    offset_to_position(source, occ.start_byte),
                    offset_to_position(source, occ.end_byte),
                ),
                new_text: new_name.to_string(),
            })
            .collect();
        changes.insert(uri, edits);
    }
    if changes.is_empty() {
        return None;
    }
    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

/// Check if a position is on a renameable identifier and return its name + range.
pub fn prepare_rename(model: &Model, source: &str, offset: usize) -> Option<(String, Range)> {
    let (name, start, end) = lang::prepare_rename(model, source, offset)?;
    Some((
        name,
        Range::new(
            offset_to_position(source, start),
            offset_to_position(source, end),
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::lang::rename::find_occurrences;
    use sysml_core::parser::parse_file;

    #[test]
    fn finds_definition_name() {
        let source = "part def Engine;\n";
        let model = parse_file("test.sysml", source);
        let occs = find_occurrences(&model, source, "Engine");
        assert_eq!(occs.len(), 1);
        assert_eq!(&source[occs[0].start_byte..occs[0].end_byte], "Engine");
    }

    #[test]
    fn finds_type_reference() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let occs = find_occurrences(&model, source, "Engine");
        assert!(
            occs.len() >= 2,
            "should find def + type ref, got {}",
            occs.len()
        );
    }

    #[test]
    fn finds_supertype_reference() {
        let source = "part def Base;\npart def Sub :> Base;\n";
        let model = parse_file("test.sysml", source);
        let occs = find_occurrences(&model, source, "Base");
        assert!(
            occs.len() >= 2,
            "should find def + supertype, got {}",
            occs.len()
        );
    }

    #[test]
    fn rename_produces_workspace_edit() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let models = vec![("file:///test.sysml", source, &model)];
        let edit = rename_symbol(&models, "Engine", "Motor");
        assert!(edit.is_some());
        let edit = edit.unwrap();
        let changes = edit.changes.unwrap();
        let uri = "file:///test.sysml".parse::<Uri>().unwrap();
        let edits = changes.get(&uri).unwrap();
        assert!(edits.len() >= 2, "should rename def + type ref");
        assert!(edits.iter().all(|e| e.new_text == "Motor"));
    }

    #[test]
    fn rename_across_files() {
        let source_a = "part def Engine;\n";
        let source_b = "part def Vehicle {\n    part engine : Engine;\n}\n";
        let model_a = parse_file("a.sysml", source_a);
        let model_b = parse_file("b.sysml", source_b);
        let models = vec![
            ("file:///a.sysml", source_a, &model_a),
            ("file:///b.sysml", source_b, &model_b),
        ];
        let edit = rename_symbol(&models, "Engine", "Motor");
        assert!(edit.is_some());
        let changes = edit.unwrap().changes.unwrap();
        assert!(changes.contains_key(&"file:///a.sysml".parse::<Uri>().unwrap()));
        assert!(changes.contains_key(&"file:///b.sysml".parse::<Uri>().unwrap()));
    }

    #[test]
    fn rename_unknown_returns_none() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let models = vec![("file:///test.sysml", source, &model)];
        let edit = rename_symbol(&models, "Unknown", "NewName");
        assert!(edit.is_none());
    }

    #[test]
    fn prepare_rename_on_def_name() {
        let source = "part def Engine;\n";
        let model = parse_file("test.sysml", source);
        let offset = source.find("Engine").unwrap();
        let result = prepare_rename(&model, source, offset);
        assert!(result.is_some());
        let (name, _range) = result.unwrap();
        assert_eq!(name, "Engine");
    }

    #[test]
    fn prepare_rename_off_identifier_returns_none() {
        let source = "part def Engine;\n";
        let model = parse_file("test.sysml", source);
        // Offset on "part" keyword, not on a renameable identifier
        let result = prepare_rename(&model, source, 0);
        assert!(result.is_none());
    }

    #[test]
    fn rename_covers_import_statements() {
        let source = "private import Pkg::Drone;\npart def Fleet {\n    part d : Drone;\n}\n";
        let model = parse_file("test.sysml", source);
        let occs = find_occurrences(&model, source, "Drone");
        let texts: Vec<&str> = occs
            .iter()
            .map(|o| &source[o.start_byte..o.end_byte])
            .collect();
        // Must include the mention inside the import statement
        let import_hit = occs
            .iter()
            .any(|o| o.start_byte < source.find(";\n").unwrap());
        assert!(
            import_hit,
            "import statement occurrence missing: {texts:?} at {occs:?}"
        );
    }

    #[test]
    fn rename_covers_satisfy_statements() {
        let source = "requirement def MaxMass;\npart def Car {\n    satisfy MaxMass;\n}\n";
        let model = parse_file("test.sysml", source);
        let occs = find_occurrences(&model, source, "MaxMass");
        assert!(
            occs.len() >= 2,
            "def + satisfy occurrence expected, got {:?}",
            occs
        );
    }

    #[test]
    fn rename_covers_connect_statements() {
        let source = "part def S {\n    part a;\n    part b;\n    connect a to b;\n}\n";
        let model = parse_file("test.sysml", source);
        let occs = find_occurrences(&model, source, "a");
        // usage decl + connect endpoint
        assert!(
            occs.len() >= 2,
            "usage + connect occurrence expected, got {:?}",
            occs
        );
    }

    #[test]
    fn word_boundary_prevents_partial_match() {
        let source = "part def EngineController;\npart def Engine;\n";
        let model = parse_file("test.sysml", source);
        let occs = find_occurrences(&model, source, "Engine");
        // Should find "Engine" def on line 2, NOT "Engine" inside "EngineController"
        assert_eq!(
            occs.len(),
            1,
            "should only match whole word, got {:?}",
            occs
        );
        assert_eq!(&source[occs[0].start_byte..occs[0].end_byte], "Engine");
    }
}
