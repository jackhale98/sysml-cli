//! Formatting as minimal edits.

use super::Edit;
use crate::codegen::format::{format_source, FormatOptions};

/// The formatted text, or `None` when `source` is already formatted.
pub fn format_text(source: &str, opts: &FormatOptions) -> Option<String> {
    let formatted = format_source(source, opts);
    (formatted != source).then_some(formatted)
}

/// Edits that format `source` (default options), touching only what
/// changes — typically the indentation of some lines.
pub fn format_edits(source: &str) -> Vec<Edit> {
    match format_text(source, &FormatOptions::default()) {
        Some(formatted) => minimal_edits(source, &formatted),
        None => Vec::new(),
    }
}

/// Small edits turning `old` into `new`: unchanged leading and trailing
/// lines are skipped and, when the line counts agree, each changed line
/// gets its own edit trimmed to the characters that differ.
pub fn minimal_edits(old: &str, new: &str) -> Vec<Edit> {
    if old == new {
        return Vec::new();
    }
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();
    let prefix = old_lines
        .iter()
        .zip(&new_lines)
        .take_while(|(a, b)| a == b)
        .count();
    let max_suffix = old_lines.len().min(new_lines.len()) - prefix;
    let suffix = old_lines
        .iter()
        .rev()
        .zip(new_lines.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    let start: usize = old_lines[..prefix].iter().map(|l| l.len()).sum();
    let old_mid = &old_lines[prefix..old_lines.len() - suffix];
    let new_mid = &new_lines[prefix..new_lines.len() - suffix];

    let mut edits = Vec::new();
    if old_mid.len() == new_mid.len() {
        let mut at = start;
        for (a, b) in old_mid.iter().zip(new_mid) {
            if a != b {
                edits.push(trimmed_edit(at, a, b));
            }
            at += a.len();
        }
    } else {
        edits.push(trimmed_edit(start, &old_mid.concat(), &new_mid.concat()));
    }
    edits
}

/// One edit replacing `old` (at byte `at`) by `new`, minus their common
/// prefix and suffix.
fn trimmed_edit(at: usize, old: &str, new: &str) -> Edit {
    let mut p = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(p) || !new.is_char_boundary(p) {
        p -= 1;
    }
    let (o, n) = (&old[p..], &new[p..]);
    let mut s = o
        .bytes()
        .rev()
        .zip(n.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !o.is_char_boundary(o.len() - s) || !n.is_char_boundary(n.len() - s) {
        s -= 1;
    }
    Edit {
        start: at + p,
        end: at + p + o.len() - s,
        insert: n[..n.len() - s].to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(source: &str, edits: &[Edit]) -> String {
        let mut out = source.to_owned();
        for e in edits.iter().rev() {
            out.replace_range(e.start..e.end, &e.insert);
        }
        out
    }

    #[test]
    fn formatting_reindents_only_changed_lines() {
        let source = "package P {\npart def A {\n    attribute x;\n}\n}\n";
        let edits = format_edits(source);
        assert_eq!(
            apply(source, &edits),
            "package P {\n    part def A {\n        attribute x;\n    }\n}\n"
        );
        assert_eq!(edits.len(), 3, "{edits:?}");
        assert!(edits.iter().all(|e| e.insert.trim().is_empty()));
        assert!(format_edits(&apply(source, &edits)).is_empty());
    }

    #[test]
    fn line_count_changes_become_one_edit() {
        let edits = minimal_edits("a\nb\nc\n", "a\nx\ny\nc\n");
        assert_eq!(apply("a\nb\nc\n", &edits), "a\nx\ny\nc\n");
        assert_eq!(edits.len(), 1);
        let edits = minimal_edits("a°b", "a°c");
        assert_eq!(
            edits,
            [Edit {
                start: 3,
                end: 4,
                insert: "c".into()
            }]
        );
        let edits = minimal_edits("x", "x\n");
        assert_eq!(apply("x", &edits), "x\n");
    }
}
