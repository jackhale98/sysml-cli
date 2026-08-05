use std::collections::HashMap;

use sysml_core::model::{simple_name, Model};
use tower_lsp_server::ls_types::{TextEdit, Uri, WorkspaceEdit};

/// A text occurrence of a name in source: byte start and byte end.
#[derive(Debug)]
struct Occurrence {
    start_byte: usize,
    end_byte: usize,
}

/// Find all byte-level occurrences of `old_name` as a complete identifier in the source,
/// scoped to spans from the model (definitions, usages, type_references).
fn find_occurrences(model: &Model, source: &str, old_name: &str) -> Vec<Occurrence> {
    let target = simple_name(old_name);
    let mut occs = Vec::new();

    // Definition names
    for def in &model.definitions {
        if def.name == target {
            let text = &source[def.span.start_byte..def.span.end_byte];
            if let Some(pos) = find_word_in(text, target) {
                occs.push(Occurrence {
                    start_byte: def.span.start_byte + pos,
                    end_byte: def.span.start_byte + pos + target.len(),
                });
            }
        }
        // Supertype reference
        if let Some(ref st) = def.super_type {
            if simple_name(st) == target {
                let text = &source[def.span.start_byte..def.span.end_byte];
                // Find the last occurrence (supertype comes after `:`/`:>`)
                if let Some(pos) = rfind_word_in(text, target) {
                    occs.push(Occurrence {
                        start_byte: def.span.start_byte + pos,
                        end_byte: def.span.start_byte + pos + target.len(),
                    });
                }
            }
        }
    }

    // Usage names and type_refs
    for usage in &model.usages {
        let text = &source[usage.span.start_byte..usage.span.end_byte];
        if usage.name == target {
            if let Some(pos) = find_word_in(text, target) {
                occs.push(Occurrence {
                    start_byte: usage.span.start_byte + pos,
                    end_byte: usage.span.start_byte + pos + target.len(),
                });
            }
        }
        if let Some(ref tr) = usage.type_ref {
            if simple_name(tr) == target {
                if let Some(pos) = rfind_word_in(text, target) {
                    let abs = usage.span.start_byte + pos;
                    // Don't duplicate if same position as usage name
                    if !occs.iter().any(|o| o.start_byte == abs) {
                        occs.push(Occurrence {
                            start_byte: abs,
                            end_byte: abs + target.len(),
                        });
                    }
                }
            }
        }
    }

    // Explicit type_references
    for tr in &model.type_references {
        if simple_name(&tr.name) == target {
            let text = &source[tr.span.start_byte..tr.span.end_byte];
            if let Some(pos) = find_word_in(text, target) {
                let abs = tr.span.start_byte + pos;
                if !occs.iter().any(|o| o.start_byte == abs) {
                    occs.push(Occurrence {
                        start_byte: abs,
                        end_byte: abs + target.len(),
                    });
                }
            }
        }
    }

    // Relationship statements and imports mention names too — a rename
    // that skips them silently corrupts traceability. Scan every
    // whole-word occurrence within each statement's span.
    let mut scan_span = |span: &sysml_core::model::Span, mentions: bool| {
        if !mentions || span.end_byte > source.len() || span.start_byte >= span.end_byte {
            return;
        }
        let text = &source[span.start_byte..span.end_byte];
        for pos in find_all_words_in(text, target) {
            let abs = span.start_byte + pos;
            if !occs.iter().any(|o| o.start_byte == abs) {
                occs.push(Occurrence {
                    start_byte: abs,
                    end_byte: abs + target.len(),
                });
            }
        }
    };

    for imp in &model.imports {
        scan_span(
            &imp.span,
            imp.path.split("::").any(|seg| seg == target),
        );
    }
    for s in &model.satisfactions {
        let mentions = simple_name(&s.requirement) == target
            || s.requirement.split("::").any(|seg| simple_name(seg) == target)
            || s.by.as_deref().map(simple_name) == Some(target);
        scan_span(&s.span, mentions);
    }
    for v in &model.verifications {
        let mentions = simple_name(&v.requirement) == target
            || v.requirement.split("::").any(|seg| simple_name(seg) == target)
            || simple_name(&v.by) == target;
        scan_span(&v.span, mentions);
    }
    for c in &model.connections {
        let mentions = [&c.source, &c.target]
            .iter()
            .any(|r| r.split(['.', ':']).any(|seg| seg == target));
        scan_span(&c.span, mentions);
    }
    for f in &model.flows {
        let mentions = [&f.source, &f.target]
            .iter()
            .any(|r| r.split(['.', ':']).any(|seg| seg == target))
            || f.item_type.as_deref().map(simple_name) == Some(target);
        scan_span(&f.span, mentions);
    }
    for a in &model.allocations {
        let mentions = [&a.source, &a.target]
            .iter()
            .any(|r| r.split(['.', ':']).any(|seg| seg == target));
        scan_span(&a.span, mentions);
    }

    // Sort and dedup
    occs.sort_by_key(|o| o.start_byte);
    occs.dedup_by_key(|o| o.start_byte);
    occs
}

/// Find all whole-word occurrences of `word` in `text`.
fn find_all_words_in(text: &str, word: &str) -> Vec<usize> {
    let mut result = Vec::new();
    let mut start = 0;
    while let Some(pos) = text[start..].find(word) {
        let abs = start + pos;
        if is_word_boundary(text, abs, word.len()) {
            result.push(abs);
        }
        start = abs + 1;
    }
    result
}

/// Find a whole-word occurrence of `word` in `text` (first match).
fn find_word_in(text: &str, word: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(pos) = text[start..].find(word) {
        let abs = start + pos;
        if is_word_boundary(text, abs, word.len()) {
            return Some(abs);
        }
        start = abs + 1;
    }
    None
}

/// Find a whole-word occurrence of `word` in `text` (last match).
fn rfind_word_in(text: &str, word: &str) -> Option<usize> {
    let mut last = None;
    let mut start = 0;
    while let Some(pos) = text[start..].find(word) {
        let abs = start + pos;
        if is_word_boundary(text, abs, word.len()) {
            last = Some(abs);
        }
        start = abs + 1;
    }
    last
}

fn is_word_boundary(text: &str, pos: usize, len: usize) -> bool {
    let before_ok = pos == 0
        || !text.as_bytes()[pos - 1].is_ascii_alphanumeric() && text.as_bytes()[pos - 1] != b'_';
    let after = pos + len;
    let after_ok = after >= text.len()
        || !text.as_bytes()[after].is_ascii_alphanumeric() && text.as_bytes()[after] != b'_';
    before_ok && after_ok
}

/// Build a WorkspaceEdit that renames `old_name` to `new_name` across all provided models.
pub fn rename_symbol(
    models: &[(&str, &str, &Model)], // (uri, source, model)
    old_name: &str,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();

    for &(uri_str, source, model) in models {
        let occs = find_occurrences(model, source, old_name);
        if occs.is_empty() {
            continue;
        }
        let uri = uri_str.parse::<Uri>().ok()?;
        let edits: Vec<TextEdit> = occs
            .iter()
            .map(|occ| {
                let start = crate::convert::offset_to_position(source, occ.start_byte);
                let end = crate::convert::offset_to_position(source, occ.end_byte);
                TextEdit {
                    range: tower_lsp_server::ls_types::Range::new(start, end),
                    new_text: new_name.to_string(),
                }
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
pub fn prepare_rename(
    model: &Model,
    source: &str,
    offset: usize,
) -> Option<(String, tower_lsp_server::ls_types::Range)> {
    // Check definitions
    for def in &model.definitions {
        if def.span.start_byte <= offset && offset < def.span.end_byte {
            let text = &source[def.span.start_byte..def.span.end_byte];
            if let Some(pos) = find_word_in(text, &def.name) {
                let abs_start = def.span.start_byte + pos;
                let abs_end = abs_start + def.name.len();
                if abs_start <= offset && offset < abs_end {
                    let start = crate::convert::offset_to_position(source, abs_start);
                    let end = crate::convert::offset_to_position(source, abs_end);
                    return Some((
                        def.name.clone(),
                        tower_lsp_server::ls_types::Range::new(start, end),
                    ));
                }
            }
        }
    }

    // Check usages
    for usage in &model.usages {
        if usage.span.start_byte <= offset && offset < usage.span.end_byte {
            let text = &source[usage.span.start_byte..usage.span.end_byte];
            if let Some(pos) = find_word_in(text, &usage.name) {
                let abs_start = usage.span.start_byte + pos;
                let abs_end = abs_start + usage.name.len();
                if abs_start <= offset && offset < abs_end {
                    let start = crate::convert::offset_to_position(source, abs_start);
                    let end = crate::convert::offset_to_position(source, abs_end);
                    return Some((
                        usage.name.clone(),
                        tower_lsp_server::ls_types::Range::new(start, end),
                    ));
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
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
