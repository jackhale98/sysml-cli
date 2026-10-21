//! Rename.
//!
//! [`rename`] renames the element under the cursor everywhere it is
//! referenced in the project, resolving each candidate name so unrelated
//! elements that share the name are left alone. The per-model functions
//! below serve the language server.

use super::index::{Src, Workspace};
use super::{lex, TextChange};
use crate::model::{simple_name, Model, Span};

/// The changes renaming the element named at byte `offset` of `src` to
/// `new_name` (quoted when it is not a basic name), or why it can't be.
pub fn rename(
    ws: &Workspace,
    src: Src,
    offset: usize,
    new_name: &str,
) -> Result<Vec<TextChange>, String> {
    let new_key = lex::unquote(new_name.trim());
    if new_key.is_empty() {
        return Err("the new name is empty".into());
    }
    if new_key.contains(['\n', '\r']) {
        return Err("a name cannot span lines".into());
    }
    let (id, _) = ws
        .resolve_at(src, offset)
        .ok_or("there is no element to rename here")?;
    let target = ws.canonical(id);
    let e = ws.elem(target);
    if matches!(target.src, Src::Library(_)) {
        return Err(format!(
            "`{}` is part of the standard library and cannot be renamed",
            ws.qualified_name(target)
        ));
    }
    if e.key == new_key {
        return Ok(Vec::new());
    }
    let clash = match ws.parent(target) {
        Some(p) => ws.children(p).any(|c| ws.elem(c).key == new_key),
        None => ws.root(&new_key).is_some(),
    };
    if clash {
        let owner = ws.parent(target).map_or_else(
            || "the root namespace".to_owned(),
            |p| format!("`{}`", ws.qualified_name(p)),
        );
        return Err(format!("{owner} already has a member named `{new_key}`"));
    }
    let insert = lex::quote_name(&new_key);
    Ok(ws
        .occurrences(target)
        .into_iter()
        .map(|(s, start, end)| TextChange {
            path: ws.file(s).path.clone(),
            start,
            end,
            insert: insert.clone(),
        })
        .collect())
}

/// A text occurrence of a name in source: byte start and byte end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrence {
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Find all byte-level occurrences of `old_name` as a complete identifier
/// in the source, scoped to spans from the model (definitions, usages,
/// type references, relationships and imports).
pub fn find_occurrences(model: &Model, source: &str, old_name: &str) -> Vec<Occurrence> {
    let target = simple_name(old_name);
    let mut occs: Vec<Occurrence> = Vec::new();
    let push_at = |occs: &mut Vec<Occurrence>, abs: usize| {
        if !occs.iter().any(|o| o.start_byte == abs) {
            occs.push(Occurrence {
                start_byte: abs,
                end_byte: abs + target.len(),
            });
        }
    };

    for def in &model.definitions {
        let text = &source[def.span.start_byte..def.span.end_byte];
        if def.name == target {
            if let Some(pos) = find_word_in(text, target) {
                push_at(&mut occs, def.span.start_byte + pos);
            }
        }
        // Supertype reference (comes after `:`/`:>`: the last occurrence)
        if def.super_type.as_deref().map(simple_name) == Some(target) {
            if let Some(pos) = rfind_word_in(text, target) {
                push_at(&mut occs, def.span.start_byte + pos);
            }
        }
    }

    for usage in &model.usages {
        let text = &source[usage.span.start_byte..usage.span.end_byte];
        if usage.name == target {
            if let Some(pos) = find_word_in(text, target) {
                push_at(&mut occs, usage.span.start_byte + pos);
            }
        }
        if usage.type_ref.as_deref().map(simple_name) == Some(target) {
            if let Some(pos) = rfind_word_in(text, target) {
                push_at(&mut occs, usage.span.start_byte + pos);
            }
        }
    }

    for tr in &model.type_references {
        if simple_name(&tr.name) == target {
            let text = &source[tr.span.start_byte..tr.span.end_byte];
            if let Some(pos) = find_word_in(text, target) {
                push_at(&mut occs, tr.span.start_byte + pos);
            }
        }
    }

    // Relationship statements and imports mention names too — a rename
    // that skips them silently corrupts traceability. Scan every
    // whole-word occurrence within each statement's span.
    let mut scan_span = |span: &Span, mentions: bool| {
        if !mentions || span.end_byte > source.len() || span.start_byte >= span.end_byte {
            return;
        }
        let text = &source[span.start_byte..span.end_byte];
        for pos in find_all_words_in(text, target) {
            push_at(&mut occs, span.start_byte + pos);
        }
    };
    let chain_mentions = |r: &str| r.split(['.', ':']).any(|seg| seg == target);
    let req_mentions =
        |r: &str| simple_name(r) == target || r.split("::").any(|seg| simple_name(seg) == target);

    for imp in &model.imports {
        scan_span(&imp.span, imp.path.split("::").any(|seg| seg == target));
    }
    for s in &model.satisfactions {
        let by = s.by.as_deref().map(simple_name) == Some(target);
        scan_span(&s.span, req_mentions(&s.requirement) || by);
    }
    for v in &model.verifications {
        scan_span(
            &v.span,
            req_mentions(&v.requirement) || simple_name(&v.by) == target,
        );
    }
    for c in &model.connections {
        scan_span(
            &c.span,
            chain_mentions(&c.source) || chain_mentions(&c.target),
        );
    }
    for f in &model.flows {
        let mentions = chain_mentions(&f.source)
            || chain_mentions(&f.target)
            || f.item_type.as_deref().map(simple_name) == Some(target);
        scan_span(&f.span, mentions);
    }
    for a in &model.allocations {
        scan_span(
            &a.span,
            chain_mentions(&a.source) || chain_mentions(&a.target),
        );
    }

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
pub fn find_word_in(text: &str, word: &str) -> Option<usize> {
    find_all_words_in(text, word).first().copied()
}

/// Find a whole-word occurrence of `word` in `text` (last match).
fn rfind_word_in(text: &str, word: &str) -> Option<usize> {
    find_all_words_in(text, word).last().copied()
}

fn is_word_boundary(text: &str, pos: usize, len: usize) -> bool {
    let bytes = text.as_bytes();
    let before_ok = pos == 0 || !bytes[pos - 1].is_ascii_alphanumeric() && bytes[pos - 1] != b'_';
    let after = pos + len;
    let after_ok =
        after >= text.len() || !bytes[after].is_ascii_alphanumeric() && bytes[after] != b'_';
    before_ok && after_ok
}

/// The occurrences of `old_name` in each `(uri, source, model)`, for the
/// files that have any.
pub fn rename_occurrences<'a>(
    models: &[(&'a str, &'a str, &Model)],
    old_name: &str,
) -> Vec<(&'a str, &'a str, Vec<Occurrence>)> {
    models
        .iter()
        .filter_map(|&(uri, source, model)| {
            let occs = find_occurrences(model, source, old_name);
            (!occs.is_empty()).then_some((uri, source, occs))
        })
        .collect()
}

/// If `offset` is on the name of a definition or usage, its name and byte
/// span.
pub fn prepare_rename(
    model: &Model,
    source: &str,
    offset: usize,
) -> Option<(String, usize, usize)> {
    let spans = model
        .definitions
        .iter()
        .map(|d| (&d.span, &d.name))
        .chain(model.usages.iter().map(|u| (&u.span, &u.name)));
    for (span, name) in spans {
        if span.start_byte <= offset && offset < span.end_byte {
            let text = &source[span.start_byte..span.end_byte];
            if let Some(pos) = find_word_in(text, name) {
                let abs_start = span.start_byte + pos;
                let abs_end = abs_start + name.len();
                if abs_start <= offset && offset < abs_end {
                    return Some((name.clone(), abs_start, abs_end));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn finds_definition_name() {
        let source = "part def Engine;\n";
        let model = parse_file("test.sysml", source);
        let occs = find_occurrences(&model, source, "Engine");
        assert_eq!(occs.len(), 1);
        assert_eq!(&source[occs[0].start_byte..occs[0].end_byte], "Engine");
    }

    #[test]
    fn finds_type_and_supertype_references() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\npart def V2 :> Vehicle;\n";
        let model = parse_file("test.sysml", source);
        assert!(find_occurrences(&model, source, "Engine").len() >= 2);
        assert!(find_occurrences(&model, source, "Vehicle").len() >= 2);
    }

    #[test]
    fn rename_covers_import_statements() {
        let source = "private import Pkg::Drone;\npart def Fleet {\n    part d : Drone;\n}\n";
        let model = parse_file("test.sysml", source);
        let occs = find_occurrences(&model, source, "Drone");
        assert!(occs
            .iter()
            .any(|o| o.start_byte < source.find(";\n").unwrap()));
    }

    #[test]
    fn rename_covers_satisfy_and_connect_statements() {
        let source = "requirement def MaxMass;\npart def Car {\n    satisfy MaxMass;\n}\n";
        let model = parse_file("test.sysml", source);
        assert!(find_occurrences(&model, source, "MaxMass").len() >= 2);
        let source = "part def S {\n    part a;\n    part b;\n    connect a to b;\n}\n";
        let model = parse_file("test.sysml", source);
        assert!(find_occurrences(&model, source, "a").len() >= 2);
    }

    #[test]
    fn word_boundary_prevents_partial_match() {
        let source = "part def EngineController;\npart def Engine;\n";
        let model = parse_file("test.sysml", source);
        let occs = find_occurrences(&model, source, "Engine");
        assert_eq!(occs.len(), 1, "{occs:?}");
    }

    #[test]
    fn prepare_rename_on_names_only() {
        let source = "part def Engine;\n";
        let model = parse_file("test.sysml", source);
        let (name, s, e) = prepare_rename(&model, source, 10).unwrap();
        assert_eq!((name.as_str(), s, e), ("Engine", 9, 15));
        assert!(prepare_rename(&model, source, 0).is_none());
    }

    #[test]
    fn project_rename_is_scope_aware_and_quotes() {
        let a = "package A {\n    private import B::*;\n    part d { part battery : Battery; }\n    part def Battery2 { part battery; }\n}\n";
        let b = "package B {\n    part def Battery;\n}\n";
        let ws = Workspace::from_sources([("a.sysml", a), ("b.sysml", b)]);
        let changes = rename(
            &ws,
            Src::Project(0),
            a.find("Battery;").unwrap(),
            "Battery pack",
        )
        .unwrap();
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|c| c.insert == "'Battery pack'"));
        assert!(changes.iter().any(|c| c.path == "b.sysml"));

        // Renaming one `battery` usage leaves the other alone.
        let changes = rename(&ws, Src::Project(0), a.find("battery").unwrap(), "pack").unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].start, a.find("battery").unwrap());
    }

    #[test]
    fn project_rename_refusals() {
        let a = "part def A; part def B; part x : A;";
        let ws = Workspace::from_sources([("a.sysml", a)]);
        let err = rename(&ws, Src::Project(0), a.find("A;").unwrap(), "B").unwrap_err();
        assert!(err.contains("already has a member named `B`"), "{err}");
        assert!(rename(&ws, Src::Project(0), 0, "Z").is_err());
        assert!(rename(&ws, Src::Project(0), 9, " ").is_err());
    }
}
