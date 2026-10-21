//! Syntax highlighting from the grammar's `highlights.scm` query.
//!
//! Produces non-overlapping byte spans tagged with capture names
//! (`keyword`, `type.definition`, `comment`, ...), for editors that colour
//! text themselves (the web app's CodeMirror, Emacs, ...).

use std::sync::OnceLock;

use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use crate::parser::get_language;

const HIGHLIGHTS_SCM: &str = sysml_syntax::HIGHLIGHTS_QUERY;

/// A highlighted byte range of the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    /// Index into [`capture_names`].
    pub capture: u32,
}

fn query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| Query::new(&get_language(), HIGHLIGHTS_SCM).ok())
        .as_ref()
}

/// The capture names spans refer to, e.g. `["comment", "keyword", ...]`.
pub fn capture_names() -> Vec<&'static str> {
    query()
        .map(|q| q.capture_names().to_vec())
        .unwrap_or_default()
}

/// Highlight `source`. Spans are sorted, non-overlapping and never empty;
/// where captures overlap, the innermost (most specific) node wins, and for
/// the same node the earliest pattern in the query wins.
pub fn highlight(source: &str) -> Vec<HighlightSpan> {
    let Some(query) = query() else {
        return Vec::new();
    };
    let mut parser = Parser::new();
    if parser.set_language(&get_language()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    // (start, end, pattern index, capture) for every capture.
    let mut raw: Vec<(usize, usize, usize, u32)> = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
    while let Some(m) = {
        matches.advance();
        matches.get()
    } {
        for capture in m.captures() {
            let node = capture.node;
            if node.start_byte() < node.end_byte() {
                raw.push((
                    node.start_byte(),
                    node.end_byte(),
                    m.pattern_index,
                    capture.index,
                ));
            }
        }
    }

    // Paint narrower spans over wider ones: process widest first, earlier
    // patterns last so they win ties, and let later writes override bytes.
    raw.sort_by(|a, b| {
        (b.1 - b.0)
            .cmp(&(a.1 - a.0))
            .then(b.2.cmp(&a.2))
            .then(a.0.cmp(&b.0))
    });
    let mut paint: Vec<Option<u32>> = vec![None; source.len()];
    for &(start, end, _, capture) in &raw {
        for slot in &mut paint[start..end] {
            *slot = Some(capture);
        }
    }

    let mut spans = Vec::new();
    let mut i = 0;
    while i < paint.len() {
        let Some(capture) = paint[i] else {
            i += 1;
            continue;
        };
        let start = i;
        while i < paint.len() && paint[i] == Some(capture) {
            i += 1;
        }
        spans.push(HighlightSpan {
            start_byte: start,
            end_byte: i,
            capture,
        });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagged(source: &str) -> Vec<(String, &'static str)> {
        let names = capture_names();
        highlight(source)
            .into_iter()
            .map(|s| {
                (
                    source[s.start_byte..s.end_byte].to_owned(),
                    names[s.capture as usize],
                )
            })
            .collect()
    }

    #[test]
    fn highlights_keywords_names_and_comments() {
        let spans = tagged("// hi\npart def Drone { part battery : Battery; }");
        assert!(spans.contains(&("// hi".into(), "comment")), "{spans:?}");
        assert!(
            spans.contains(&("Drone".into(), "type.definition")),
            "{spans:?}"
        );
        assert!(spans.contains(&("battery".into(), "variable")), "{spans:?}");
        assert!(
            spans
                .iter()
                .any(|(t, c)| t == "part" && c.starts_with("keyword")),
            "{spans:?}"
        );
    }

    #[test]
    fn spans_are_sorted_and_disjoint() {
        let spans = highlight("package P { part def A { attribute x : Real = 1.0; } }");
        assert!(!spans.is_empty());
        for pair in spans.windows(2) {
            assert!(pair[0].end_byte <= pair[1].start_byte);
        }
    }
}
