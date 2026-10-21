//! Quick fixes for diagnostics.
//!
//! [`code_actions`] offers, for the project diagnostics in a range: name
//! corrections for unresolved names, an import (or a qualified name) for a
//! name declared in another package, and removal of an unused definition.
//! The message helpers are shared with the language server.

use std::collections::HashSet;

use serde::Serialize;

use super::index::{ElemId, Src, Workspace};
use super::{lex, TextChange};
use crate::checks::references::levenshtein;
use crate::diagnostic::{codes, Diagnostic};
use crate::model::simple_name;

/// A quick fix: a title and the changes it applies.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeAction {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<String>,
    pub changes: Vec<TextChange>,
}

/// Quick fixes for the `diagnostics` of file `src` that overlap bytes
/// `start..end`.
pub fn code_actions(
    ws: &Workspace,
    src: Src,
    start: usize,
    end: usize,
    diagnostics: &[Diagnostic],
) -> Vec<CodeAction> {
    let mut actions = Vec::new();
    let mut titles = HashSet::new();
    for d in diagnostics {
        if d.span.start_byte > end || d.span.end_byte < start {
            continue;
        }
        let mut found = Vec::new();
        match d.code {
            codes::UNRESOLVED_TYPE | codes::UNRESOLVED_TARGET => {
                unresolved_fixes(ws, src, d, &mut found);
            }
            codes::UNUSED_DEF => found.extend(remove_unused(ws, src, d)),
            _ => {
                if let Some(fix) = suggestion_fix(ws, src, d) {
                    found.push(fix);
                }
            }
        }
        for mut a in found {
            a.diagnostic_code = Some(d.code.to_owned());
            if titles.insert(a.title.clone()) {
                actions.push(a);
            }
        }
    }
    actions
}

/// The name token (last segment) in the diagnostic's span that spells
/// `name`, and the token index of the path's first segment.
fn find_name(ws: &Workspace, src: Src, d: &Diagnostic, name: &str) -> Option<(usize, usize)> {
    let file = ws.file(src);
    let segs: Vec<String> = name
        .split("::")
        .flat_map(|s| s.split('.'))
        .map(lex::unquote)
        .collect();
    let first = lex::unquote(segs.first()?);
    let tokens = &file.lexed.tokens;
    let lo = file.lexed.token_after(d.span.start_byte);
    for i in lo..tokens.len() {
        let t = tokens[i];
        if t.start >= d.span.end_byte {
            break;
        }
        if !t.is_name() || lex::unquote(t.text(&file.text)) != first {
            continue;
        }
        let path = ws.path_at(src, i);
        if path.cursor != 0 {
            continue;
        }
        let names: Vec<String> = path
            .segs
            .iter()
            .map(|&s| lex::unquote(tokens[s].text(&file.text)))
            .collect();
        if names.len() >= segs.len() && names[..segs.len()] == segs[..] {
            return Some((path.segs[segs.len() - 1], i));
        }
    }
    None
}

fn replace_token(ws: &Workspace, src: Src, tok: usize, insert: String) -> TextChange {
    let file = ws.file(src);
    let t = file.lexed.tokens[tok];
    TextChange {
        path: file.path.clone(),
        start: t.start,
        end: t.end,
        insert,
    }
}

/// Fixes for W004/W005: near names, imports, qualification.
fn unresolved_fixes(ws: &Workspace, src: Src, d: &Diagnostic, out: &mut Vec<CodeAction>) {
    let Some(name) = extract_backtick_name(&d.message) else {
        return;
    };
    let Some((last_tok, first_tok)) = find_name(ws, src, d, name) else {
        return;
    };
    let file = ws.file(src);
    let tokens = &file.lexed.tokens;
    let first = lex::unquote(tokens[first_tok].text(&file.text));
    let scope = ws.scope_at(src, tokens[first_tok].start);

    // 1. Declared in another package: import it, or qualify it.
    if ws.lookup(src, scope, &first).is_none() {
        let mut candidates: Vec<ElemId> = ws
            .project_named(&first)
            .iter()
            .copied()
            .filter(|&c| ws.parent(c).is_some_and(|p| ws.elem(p).kind.is_package()))
            .collect();
        candidates.extend(
            ws.library_named(&first)
                .iter()
                .copied()
                .filter(|&c| ws.parent(c).is_some_and(|p| ws.elem(p).kind.is_package())),
        );
        for c in candidates.into_iter().take(3) {
            let qn = ws.qualified_name(c);
            if let Some(change) = import_change(ws, src, tokens[first_tok].start, &qn) {
                out.push(CodeAction {
                    title: format!("Import `{qn}`"),
                    diagnostic_code: None,
                    changes: vec![change],
                });
            }
            out.push(CodeAction {
                title: format!("Use the qualified name `{qn}`"),
                diagnostic_code: None,
                changes: vec![replace_token(ws, src, first_tok, qn)],
            });
        }
    }

    // 2. A typo: the closest visible names.
    let wrong = lex::unquote(tokens[last_tok].text(&file.text));
    let mut near: Vec<String> = Vec::new();
    if let Some(s) = d.suggestion.as_deref().and_then(extract_did_you_mean) {
        near.push(s);
    }
    let pool = ws
        .visible(src, scope)
        .into_iter()
        .chain((0..ws.files.len()).flat_map(|f| {
            let src = Src::Project(f as u32);
            (0..ws.file(src).elements.len()).map(move |e| ElemId {
                src,
                elem: e as u32,
            })
        }));
    let max = (wrong.chars().count() / 3).clamp(1, 3);
    let mut scored: Vec<(usize, String)> = pool
        .filter_map(|id| {
            let key = &ws.elem(id).key;
            let dist = levenshtein(&wrong.to_lowercase(), &key.to_lowercase());
            (!key.is_empty() && *key != wrong && dist <= max).then(|| (dist, key.clone()))
        })
        .collect();
    scored.sort();
    for (_, key) in scored {
        if near.len() >= 3 {
            break;
        }
        if !near.contains(&key) {
            near.push(key);
        }
    }
    for n in near {
        let insert = lex::quote_name(&n);
        out.push(CodeAction {
            title: format!("Change to `{insert}`"),
            diagnostic_code: None,
            changes: vec![replace_token(ws, src, last_tok, insert)],
        });
    }
}

/// A "did you mean" fix for any other diagnostic that suggests a name.
fn suggestion_fix(ws: &Workspace, src: Src, d: &Diagnostic) -> Option<CodeAction> {
    let suggestion = d.suggestion.as_deref().and_then(extract_did_you_mean)?;
    let wrong = extract_backtick_name(&d.message)?;
    let (tok, _) = find_name(ws, src, d, simple_name(wrong))?;
    Some(CodeAction {
        title: format!("Change to `{suggestion}`"),
        diagnostic_code: None,
        changes: vec![replace_token(ws, src, tok, suggestion)],
    })
}

/// Insert `private import <path>;` into the package around `offset` —
/// after its last import, else after its doc comment, else at the top of
/// its body — or at the top of the file outside any package.
fn import_change(ws: &Workspace, src: Src, offset: usize, path: &str) -> Option<TextChange> {
    let file = ws.file(src);
    let text = &file.text;
    let mut pkg = file.innermost(offset);
    while let Some(p) = pkg {
        if file.elements[p as usize].kind.is_package() {
            break;
        }
        pkg = file.elements[p as usize].parent;
    }
    let change = |at: usize, insert: String| {
        Some(TextChange {
            path: file.path.clone(),
            start: at,
            end: at,
            insert,
        })
    };
    let Some(pkg) = pkg else {
        let at = file
            .imports
            .iter()
            .filter(|i| i.owner.is_none())
            .map(|i| i.end)
            .max();
        return match at {
            Some(at) => change(at, format!("\nimport {path};")),
            None => change(0, format!("import {path};\n\n")),
        };
    };
    let pe = &file.elements[pkg as usize];
    if let Some(last) = file
        .imports
        .iter()
        .filter(|i| i.owner == Some(pkg))
        .max_by_key(|i| i.end)
    {
        return change(
            last.end,
            format!("\n{}private import {path};", indent_of(text, last.start)),
        );
    }
    let (body_start, _) = pe.body?;
    let indent = format!("{}    ", indent_of(text, pe.start));
    // After a leading `doc /* ... */`.
    let mut at = body_start + 1;
    let first = file.lexed.token_after(at);
    if let Some(t) = file.lexed.tokens.get(first) {
        if t.is_keyword(text, "doc") {
            let next_tok = file
                .lexed
                .tokens
                .get(first + 1)
                .map_or(text.len(), |n| n.start);
            if let Some(c) = file
                .lexed
                .comments
                .iter()
                .find(|c| c.block && c.start >= t.end && c.start < next_tok)
            {
                at = c.end;
            }
        }
    }
    change(at, format!("\n{indent}private import {path};"))
}

/// The leading whitespace of the line containing `offset`.
fn indent_of(text: &str, offset: usize) -> &str {
    let line_start = text[..offset].rfind('\n').map_or(0, |n| n + 1);
    let line = &text[line_start..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// Remove an unused definition (its whole lines).
fn remove_unused(ws: &Workspace, src: Src, d: &Diagnostic) -> Option<CodeAction> {
    let file = ws.file(src);
    let text = &file.text;
    let name = extract_backtick_name(&d.message).unwrap_or("definition");
    let (start, end) = full_lines(text, d.span.start_byte, d.span.end_byte);
    Some(CodeAction {
        title: format!("Remove unused `{name}`"),
        diagnostic_code: None,
        changes: vec![TextChange {
            path: file.path.clone(),
            start,
            end,
            insert: String::new(),
        }],
    })
}

/// Bytes `start..end` widened to whole lines (with the final newline).
pub fn full_lines(text: &str, start: usize, end: usize) -> (usize, usize) {
    let s = text[..start.min(text.len())]
        .rfind('\n')
        .map_or(0, |n| n + 1);
    let e = text[end.min(text.len())..]
        .find('\n')
        .map_or(text.len(), |n| end + n + 1);
    (s, e)
}

/// Parse a "did you mean `X`?" suggestion from a message.
pub fn extract_did_you_mean(message: &str) -> Option<String> {
    let marker = "did you mean `";
    let start = message.find(marker)? + marker.len();
    let rest = &message[start..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

/// The first `backtick-quoted` name in a message.
pub fn extract_backtick_name(message: &str) -> Option<&str> {
    let start = message.find('`')? + 1;
    let rest = &message[start..];
    let end = rest.find('`')?;
    Some(&rest[..end])
}

/// The line to insert a new import at: after the last import line.
pub fn find_import_insert_line(source: &str) -> u32 {
    let mut last_import_line = 0u32;
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") || trimmed.starts_with("private import ") {
            last_import_line = (i + 1) as u32;
        }
    }
    last_import_line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::analyze_models;
    use crate::parser::parse_file;

    fn apply(text: &str, changes: &[TextChange]) -> String {
        let mut out = text.to_owned();
        let mut sorted = changes.to_vec();
        sorted.sort_by_key(|c| std::cmp::Reverse(c.start));
        for c in sorted {
            out.replace_range(c.start..c.end, &c.insert);
        }
        out
    }

    fn actions_for(files: &[(&str, &str)], file: usize, at: &str) -> Vec<CodeAction> {
        let ws = Workspace::from_sources(files.iter().copied());
        let models = files.iter().map(|(p, t)| parse_file(p, t)).collect();
        let diags = analyze_models(models).swap_remove(file).1;
        let offset = files[file].1.find(at).unwrap();
        code_actions(&ws, Src::Project(file as u32), offset, offset, &diags)
    }

    #[test]
    fn message_helpers() {
        assert_eq!(
            extract_did_you_mean("Suggestion: did you mean `Vehicle`?").as_deref(),
            Some("Vehicle")
        );
        assert_eq!(extract_backtick_name("def `A` x"), Some("A"));
        assert_eq!(full_lines("a\nbcd\ne", 3, 4), (2, 6));
    }

    #[test]
    fn unresolved_type_offers_an_import_and_near_names() {
        let a = "package A {\n    doc /* About A. */\n    part d { part b : Battery; part c : Batery; }\n}\n";
        let b = "package B {\n    part def Battery;\n}\n";
        let actions = actions_for(&[("a.sysml", a), ("b.sysml", b)], 0, "Battery;");
        let import = actions
            .iter()
            .find(|a| a.title == "Import `B::Battery`")
            .unwrap_or_else(|| panic!("{actions:#?}"));
        assert_eq!(import.diagnostic_code.as_deref(), Some("W004"));
        assert_eq!(
            apply(a, &import.changes),
            "package A {\n    doc /* About A. */\n    private import B::Battery;\n    part d { part b : Battery; part c : Batery; }\n}\n"
        );
        assert!(actions
            .iter()
            .any(|a| a.title == "Use the qualified name `B::Battery`"));

        let typo = actions_for(&[("a.sysml", a), ("b.sysml", b)], 0, "Batery");
        let change = typo
            .iter()
            .find(|a| a.title == "Change to `Battery`")
            .unwrap_or_else(|| panic!("{typo:#?}"));
        assert!(apply(a, &change.changes).contains("part c : Battery;"));
    }

    #[test]
    fn import_goes_after_existing_imports() {
        let a = "package A {\n    private import C::*;\n    part d { part b : Battery; }\n}\n";
        let b = "package B { part def Battery; }\npackage C { part def X; }\n";
        let actions = actions_for(&[("a.sysml", a), ("b.sysml", b)], 0, "Battery;");
        let import = actions
            .iter()
            .find(|a| a.title.starts_with("Import"))
            .unwrap();
        assert!(apply(a, &import.changes)
            .contains("private import C::*;\n    private import B::Battery;\n"));
    }

    #[test]
    fn unused_definition_can_be_removed() {
        let a = "package A {\n    part def Unused;\n}\n";
        let actions = actions_for(&[("a.sysml", a)], 0, "Unused");
        let remove = actions
            .iter()
            .find(|a| a.title == "Remove unused `Unused`")
            .unwrap();
        assert_eq!(apply(a, &remove.changes), "package A {\n}\n");
    }
}
