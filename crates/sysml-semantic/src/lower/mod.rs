//! Lowering: concrete syntax tree → element graph.
//!
//! The tree-sitter grammar is deliberately flat and lenient (one rule covers
//! many productions, modifiers precede the declaration as siblings), so the
//! lowering reconstructs the abstract syntax from the token sequence of each
//! node, following the productions of the KerML (8.2) and SysML (8.2.2)
//! textual notation: every `ownedRelationship += ...` in the grammar becomes
//! an owned relationship here, in textual order, so element paths — and
//! therefore element ids (KerML 9.1) — match those of the reference
//! implementation. Qualified names become [`Reference`]s resolved later by
//! [`crate::resolve`].

mod behavior;
mod decl;
mod expr;

use tree_sitter::Node;

use crate::graph::{
    Element, ElementRef, End, File, Flags, Graph, Reference, Role, Span, Target, Value, Visibility,
};
use crate::metaclass::Metaclass;
use crate::name::unescape_name;

/// Lower one parsed file into `graph`, returning the root namespace.
pub(crate) fn lower_file(graph: &mut Graph, path: &str, text: &str, root: Node) -> ElementRef {
    let kerml = path.ends_with(".kerml");
    let file = graph.next_file_id();
    let mut lowerer = Lowerer {
        g: graph,
        src: text,
        file: file.0,
        kerml,
    };
    let span = lowerer.span(root);
    let ns = lowerer.g.push(Element::new(Metaclass::Namespace, span));
    lowerer.body(ns, root);
    let id = lowerer.g.add_file(File {
        path: path.into(),
        root: ns,
        kerml,
    });
    debug_assert_eq!(id, file);
    ns
}

/// Modifiers, visibility and prefix metadata written before a declaration
/// (they are siblings of the declaration node in the CST).
#[derive(Default, Clone)]
pub(crate) struct Prefix<'t> {
    pub visibility: Option<Visibility>,
    pub modifiers: Vec<&'static str>,
    pub metadata: Vec<Node<'t>>,
    pub start: Option<usize>,
}

impl Prefix<'_> {
    pub fn has(&self, modifier: &str) -> bool {
        self.modifiers.contains(&modifier)
    }
    pub fn is_empty(&self) -> bool {
        self.visibility.is_none() && self.modifiers.is_empty() && self.metadata.is_empty()
    }
}

pub(crate) struct Lowerer<'g, 's> {
    pub g: &'g mut Graph,
    pub src: &'s str,
    pub file: u32,
    /// KerML file (`.kerml`): metaclasses default to their KerML forms.
    pub kerml: bool,
}

/// All children of a node, named and anonymous, excluding comments.
pub(crate) fn kids<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| !matches!(c.kind(), "line_comment" | "block_comment"))
        .collect()
}

/// All children of a node including comments.
pub(crate) fn all_kids<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

impl<'g, 's> Lowerer<'g, 's> {
    pub fn text(&self, node: Node) -> &'s str {
        &self.src[node.start_byte()..node.end_byte()]
    }

    pub fn span(&self, node: Node) -> Span {
        Span {
            file: self.file,
            start: node.start_byte() as u32,
            end: node.end_byte() as u32,
        }
    }

    pub fn span_from(&self, start: usize, node: Node) -> Span {
        Span {
            file: self.file,
            start: start.min(node.start_byte()) as u32,
            end: node.end_byte() as u32,
        }
    }

    /// A new, unowned element.
    pub fn new_el(&mut self, kind: Metaclass, span: Span) -> ElementRef {
        self.g.push(Element::new(kind, span))
    }

    pub fn el(&mut self, e: ElementRef) -> &mut Element {
        self.g.get_mut(e)
    }

    pub fn kind_of(&self, e: ElementRef) -> Metaclass {
        self.g.get(e).kind
    }

    /// Create a relationship of `kind` owned by `owner`.
    pub fn rel(&mut self, owner: ElementRef, kind: Metaclass, span: Span) -> ElementRef {
        let r = self.new_el(kind, span);
        self.g.attach(owner, r);
        r
    }

    /// Create a membership of `kind` in `ns` owning `member`.
    pub fn own_member(
        &mut self,
        ns: ElementRef,
        kind: Metaclass,
        member: ElementRef,
        visibility: Option<Visibility>,
    ) -> ElementRef {
        let span = self.g.get(member).span;
        let m = self.rel(ns, kind, span);
        if let Some(v) = visibility {
            let el = self.el(m);
            el.visibility = v;
            el.flags.insert(Flags::VISIBILITY_WRITTEN);
        }
        self.g.attach(m, member);
        m
    }

    /// The name an `identifier` (or `quoted_name`) node denotes.
    pub fn name(&self, node: Node) -> Box<str> {
        let text = self.text(node).trim();
        unescape_name(text).into()
    }

    /// Set declared name / short name of `e` from a `short_name` node or a
    /// `name:` identifier.
    pub fn set_name(&mut self, e: ElementRef, node: Node) {
        let name = self.name(node);
        let span = (node.start_byte() as u32, node.end_byte() as u32);
        let el = self.el(e);
        el.name = Some(name);
        el.name_span = Some(span);
    }

    pub fn set_short_name(&mut self, e: ElementRef, node: Node) {
        // short_name: '<' quoted_name '>'
        let inner = kids(node)
            .into_iter()
            .find(|k| k.kind() == "quoted_name")
            .unwrap_or(node);
        let name = self.name(inner);
        let span = (inner.start_byte() as u32, inner.end_byte() as u32);
        let el = self.el(e);
        el.short_name = Some(name);
        if el.name_span.is_none() {
            el.name_span = Some(span);
        }
    }

    /// A [`Reference`] for a `qualified_name` or `identifier` node.
    pub fn reference(&mut self, owner: ElementRef, node: Node) -> crate::graph::RefId {
        let mut segments = Vec::new();
        let mut spans = Vec::new();
        let mut global = false;
        match node.kind() {
            "qualified_name" => {
                for k in kids(node) {
                    match k.kind() {
                        "identifier" => {
                            segments.push(self.name(k));
                            spans.push((k.start_byte() as u32, k.end_byte() as u32));
                        }
                        "$" => global = true,
                        _ => {}
                    }
                }
            }
            _ => {
                segments.push(self.name(node));
                spans.push((node.start_byte() as u32, node.end_byte() as u32));
            }
        }
        let span = self.span(node);
        self.g.add_ref(Reference {
            segments,
            segment_spans: spans,
            global,
            span,
            owner,
        })
    }

    /// Point `rel`'s end with `role` at the element named by `node`: a
    /// qualified name becomes a [`Target::Name`]; a feature chain becomes an
    /// owned `Feature` with one `FeatureChaining` per segment
    /// ([`Target::Owned`]).
    pub fn set_end(&mut self, rel: ElementRef, role: Role, node: Node) {
        let target = self.target(rel, node);
        self.el(rel).ends.push(End { role, target });
    }

    pub fn target(&mut self, rel: ElementRef, node: Node) -> Target {
        if node.kind() == "feature_chain" {
            let chain = self.feature_chain(node);
            self.g.attach(rel, chain);
            Target::Owned(chain)
        } else {
            Target::Name(self.reference(rel, node))
        }
    }

    /// An (unowned) `Feature` with `FeatureChaining`s for a `feature_chain`
    /// node (`a::b.c.d`).
    pub fn feature_chain(&mut self, node: Node) -> ElementRef {
        let span = self.span(node);
        let chain = self.new_el(Metaclass::Feature, span);
        self.chainings(chain, node);
        chain
    }

    /// Add `FeatureChaining`s to `chain` for each segment of `node`.
    pub fn chainings(&mut self, chain: ElementRef, node: Node) {
        for k in kids(node) {
            if matches!(k.kind(), "qualified_name" | "identifier") {
                let span = self.span(k);
                let r = self.rel(chain, Metaclass::FeatureChaining, span);
                let target = Target::Name(self.reference(r, k));
                self.el(r).ends.push(End {
                    role: Role::Target,
                    target,
                });
            } else if k.kind() == "feature_chain" {
                self.chainings(chain, k);
            }
        }
    }

    /// The processed body of a comment (KerML 8.2.3.3.2, Note 1).
    pub fn comment_body(&self, node: Node) -> Box<str> {
        comment_body(self.text(node)).into()
    }

    pub fn set_body(&mut self, e: ElementRef, body: Box<str>, locale: Option<Box<str>>) {
        self.el(e).value = Value::Body {
            body,
            locale,
            language: None,
        };
    }

    /// Unescaped value of a `string_literal` node.
    pub fn string_value(&self, node: Node) -> Box<str> {
        unescape_string(self.text(node)).into()
    }
}

/// Strip `/*` `*/` and leading `*` decoration from a comment.
pub(crate) fn comment_body(text: &str) -> String {
    let inner = text
        .strip_prefix("//*")
        .or_else(|| text.strip_prefix("/*"))
        .unwrap_or(text);
    let inner = inner.strip_suffix("*/").unwrap_or(inner);
    // Remove white space after the initial /*, up to and including the
    // first line terminator.
    let mut rest = inner;
    let trimmed = rest.trim_start_matches([' ', '\t', '\u{c}']);
    // If a line terminator follows the opening `/*`, every remaining line
    // is a "subsequent" line and gets its decoration stripped.
    let mut first = true;
    if let Some(after) = trimmed
        .strip_prefix("\r\n")
        .or_else(|| trimmed.strip_prefix('\n'))
        .or_else(|| trimmed.strip_prefix('\r'))
    {
        rest = after;
        first = false;
    } else {
        rest = trimmed;
    }
    let mut out = String::with_capacity(rest.len());
    let mut started = false;
    for line in split_lines(rest) {
        if started {
            out.push('\n');
        }
        started = true;
        let (content, is_first) = (line, first);
        first = false;
        let mut l = content;
        if !is_first {
            l = l.trim_start_matches([' ', '\t', '\u{c}']);
            if let Some(s) = l.strip_prefix('*') {
                l = s;
                if let Some(s) = l.strip_prefix(' ') {
                    l = s;
                }
            }
        }
        out.push_str(l);
    }
    out
}

fn split_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l))
}

/// Unescape a `"..."` string literal.
pub(crate) fn unescape_string(text: &str) -> String {
    let inner = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('\\') => out.push('\\'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_bodies() {
        assert_eq!(
            comment_body("/*\n * This package defines\n * things.\n */"),
            "This package defines\nthings.\n"
        );
        assert_eq!(comment_body("/* d */"), "d ");
        assert_eq!(comment_body("/*x*/"), "x");
    }

    #[test]
    fn strings() {
        assert_eq!(unescape_string(r#""a\"b\n""#), "a\"b\n");
    }
}
