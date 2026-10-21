//! Declaration edits: change what an element's declaration says — its
//! type, specializations, redefinition, multiplicity, value, direction and
//! other modifiers, short name and doc — as a minimal text patch.
//!
//! [`set_declaration`] rewrites only the parts of the declaration that
//! change, in place, and keeps everything else (comments, layout, the
//! body). A part that is missing is inserted where the SysML v2 textual
//! notation puts it:
//!
//! ```text
//! visibility  direction abstract readonly derived end ref  #tags
//!     keyword <'short'> name [multiplicity] : Type :> subsets :>> redefines = value
//! ```
//!
//! An existing part keeps its position and spelling: a multiplicity
//! written after the type (`: Rotor[4]`) stays there, `specializes A`
//! stays a keyword, `default x` keeps its operator unless `valueKind`
//! changes it. Setting a value the declaration already has changes
//! nothing.
//!
//! [`element_details`] reads the same parts back, so an inspector can show
//! (and round-trip) exactly what the declaration says.

use serde::{Deserialize, Deserializer, Serialize};
use tree_sitter::Node;

use super::edit::{EditError, EditPlan, TextEdit};
use super::field_edit::{
    body_of, child_of_kind, children, declared_names, err, locate, parse, removal, set_doc,
    set_short_name, text,
};
use crate::model::unquote_name;
use crate::view_edit::format_name;

// --- request and result ---------------------------------------------------------------

/// `null` in JSON as `Some(None)`, a missing field as `None`.
fn nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// What to change in a declaration. Every field is optional: a missing one
/// is left as it is; `null` (or an empty string, `false`, `[]`) removes it.
#[derive(Deserialize, Default, Debug, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeclarationSet {
    /// `: T` of a usage.
    #[serde(default, rename = "type", deserialize_with = "nullable")]
    pub ty: Option<Option<String>>,
    /// `: ~T` — the conjugated type (ports).
    #[serde(default)]
    pub conjugated: Option<bool>,
    /// `:> A, B`: specializations of a definition, subsettings of a usage.
    #[serde(default)]
    pub specializes: Option<Vec<String>>,
    /// `:>> x`.
    #[serde(default, deserialize_with = "nullable")]
    pub redefines: Option<Option<String>>,
    /// `[4]`, `[0..1]`, `[1..*]` (brackets optional).
    #[serde(default, deserialize_with = "nullable")]
    pub multiplicity: Option<Option<String>>,
    /// The expression after `=`, `default =` or `:=`.
    #[serde(default, deserialize_with = "nullable")]
    pub value: Option<Option<String>>,
    /// How the value is given; a new value is bound (`=`) unless this says.
    #[serde(default)]
    pub value_kind: Option<ValueKind>,
    #[serde(default, deserialize_with = "nullable")]
    pub direction: Option<Option<Direction>>,
    #[serde(default, rename = "abstract")]
    pub is_abstract: Option<bool>,
    #[serde(default)]
    pub readonly: Option<bool>,
    #[serde(default)]
    pub derived: Option<bool>,
    #[serde(default)]
    pub end: Option<bool>,
    /// `<'ID'>`, unquoted.
    #[serde(default, deserialize_with = "nullable")]
    pub short_name: Option<Option<String>>,
    /// The first `doc /* ... */` comment's text.
    #[serde(default, deserialize_with = "nullable")]
    pub doc: Option<Option<String>>,
}

/// How a value is given: `= v` (bound), `default = v` or `:= v` (initial).
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ValueKind {
    Bind,
    Default,
    Initial,
}

impl ValueKind {
    fn operator(self) -> &'static str {
        match self {
            Self::Bind => "= ",
            Self::Default => "default = ",
            Self::Initial => ":= ",
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    In,
    Out,
    Inout,
}

impl Direction {
    fn keyword(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
            Self::Inout => "inout",
        }
    }
}

/// What an element's declaration says, read from the source.
#[derive(Serialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ElementDetails {
    /// The name as written (quoted when needed).
    pub name: String,
    /// The short name, unquoted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_name: Option<String>,
    /// The engine's kind (`part`, `port`, `use_case`, `package`, ...), as
    /// in `analyze`'s model.
    pub kind: String,
    /// A definition (or a package, which the model lists with them).
    pub is_definition: bool,
    /// The type of a usage as written, without `~`.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub conjugated: bool,
    /// Specializations (definitions) or subsettings (usages), as written.
    pub specializes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redefines: Option<String>,
    /// `[...]` without `ordered`/`nonunique`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplicity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_kind: Option<ValueKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    #[serde(rename = "abstract", skip_serializing_if = "std::ops::Not::not")]
    pub is_abstract: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub readonly: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub derived: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub end: bool,
    /// The first doc comment's text, without `/*`, `*/` and ` * `.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// `A::B::c`, each segment quoted when needed.
    pub qualified_name: String,
}

// --- the declaration's parts --------------------------------------------------------------

/// The parts of a declaration in canonical order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Slot {
    Visibility,
    Direction,
    Abstract,
    Readonly,
    Derived,
    End,
    Ref,
    Modifier,
    Tag,
    Keyword,
    ShortName,
    Name,
    Multiplicity,
    Qualifier,
    Typing,
    Specialization,
    Redefinition,
    Relationship,
    Value,
    Trailing,
    Body,
}

impl Slot {
    fn rank(self) -> u16 {
        self as u16
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Package,
    Definition,
    Usage,
}

const DEFINITIONS: [&str; 4] = [
    "definition",
    "state_definition",
    "enumeration_definition",
    "generic_definition",
];
const PACKAGES: [&str; 2] = ["package_declaration", "namespace_declaration"];
const USAGES: [&str; 11] = [
    "usage",
    "action_usage",
    "state_usage",
    "requirement_usage",
    "end_feature",
    "feature_usage",
    "connection_usage",
    "interface_usage",
    "constraint_usage",
    "allocation_usage",
    "flow_usage",
];
/// Usages whose grammar takes a short name.
const SHORT_NAMED: [&str; 2] = ["usage", "requirement_usage"];
/// Usages whose grammar takes a value.
const VALUED: [&str; 5] = [
    "usage",
    "action_usage",
    "requirement_usage",
    "end_feature",
    "feature_usage",
];
/// Usages that are features with a direction.
const DIRECTED: [&str; 4] = ["usage", "action_usage", "end_feature", "feature_usage"];
/// Keywords of a `usage` that `end` may precede.
const END_KEYWORDS: [&str; 5] = ["part", "attribute", "port", "item", "occurrence"];

fn family(kind: &str) -> Option<Family> {
    if PACKAGES.contains(&kind) {
        Some(Family::Package)
    } else if DEFINITIONS.contains(&kind) {
        Some(Family::Definition)
    } else if USAGES.contains(&kind) {
        Some(Family::Usage)
    } else {
        None
    }
}

fn is_comment(node: &Node) -> bool {
    matches!(node.kind(), "line_comment" | "block_comment")
}

fn modifier_slot(word: &str) -> Slot {
    match word {
        "in" | "out" | "inout" => Slot::Direction,
        "abstract" | "variation" => Slot::Abstract,
        "readonly" | "constant" | "const" => Slot::Readonly,
        "derived" => Slot::Derived,
        "ref" => Slot::Ref,
        _ => Slot::Modifier,
    }
}

fn prefix_slot(node: &Node, src: &str) -> Slot {
    match node.kind() {
        "visibility" => Slot::Visibility,
        "hash_tag" => Slot::Tag,
        _ => modifier_slot(text(node, src).trim()),
    }
}

#[derive(Clone, Copy)]
struct Part<'t> {
    slot: Slot,
    /// Includes the `:` before a typing, specialization or redefinition.
    start: usize,
    end: usize,
    node: Node<'t>,
}

struct Header<'t> {
    node: Node<'t>,
    family: Family,
    /// In source order: prefix modifiers, then the node's own parts.
    parts: Vec<Part<'t>>,
}

impl<'t> Header<'t> {
    fn read(node: Node<'t>, src: &str) -> Result<Self, EditError> {
        let family = family(node.kind()).ok_or_else(|| {
            err(format!(
                "a `{}` declaration cannot be edited this way",
                node.kind().replace('_', " ")
            ))
        })?;
        let mut parts = Vec::new();
        // `_declaration` puts the prefix modifiers before the node, as
        // siblings.
        let mut prefix = Vec::new();
        let mut sib = node.prev_sibling();
        while let Some(s) = sib {
            match s.kind() {
                "modifier" | "visibility" | "hash_tag" => prefix.push(s),
                _ if is_comment(&s) => {}
                _ => break,
            }
            sib = s.prev_sibling();
        }
        for s in prefix.into_iter().rev() {
            parts.push(Part {
                slot: prefix_slot(&s, src),
                start: s.start_byte(),
                end: s.end_byte(),
                node: s,
            });
        }
        let mut cursor = node.walk();
        let mut colon = None;
        let mut core = false;
        if cursor.goto_first_child() {
            loop {
                let c = cursor.node();
                let field = cursor.field_name();
                let slot = match c.kind() {
                    _ if is_comment(&c) => None,
                    "modifier" | "visibility" | "hash_tag" if !core => Some(prefix_slot(&c, src)),
                    "end" if !c.is_named() && !core => Some(Slot::End),
                    "short_name" => Some(Slot::ShortName),
                    "identifier" if matches!(field, Some("name" | "end_name")) => Some(Slot::Name),
                    "multiplicity" => Some(Slot::Multiplicity),
                    "ordered" | "nonunique" if !c.is_named() => Some(Slot::Qualifier),
                    ":" if !c.is_named() => {
                        colon = Some(c.start_byte());
                        None
                    }
                    "typed_by" | "typed_by_keyword" => Some(Slot::Typing),
                    "specialization" => Some(Slot::Specialization),
                    "redefinition" => Some(Slot::Redefinition),
                    "keyword_type_relationship" => {
                        Some(match c.child(0).map(|k| k.kind()).unwrap_or("") {
                            "specializes" | "subsets" => Slot::Specialization,
                            "redefines" => Slot::Redefinition,
                            _ => Slot::Relationship,
                        })
                    }
                    "binding" | "inverse_keyword" | "tilde_conjugation" => Some(Slot::Relationship),
                    "value_assignment" => Some(Slot::Value),
                    ";" => Some(Slot::Body),
                    k if k.ends_with("_body") => Some(Slot::Body),
                    _ if !c.is_named() && !core => Some(Slot::Keyword),
                    _ => Some(Slot::Trailing),
                };
                if let Some(slot) = slot {
                    core |= slot >= Slot::ShortName;
                    let start = match slot {
                        Slot::Typing | Slot::Specialization | Slot::Redefinition => {
                            colon.take().unwrap_or(c.start_byte())
                        }
                        _ => c.start_byte(),
                    };
                    parts.push(Part {
                        slot,
                        start,
                        end: c.end_byte(),
                        node: c,
                    });
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        Ok(Self {
            node,
            family,
            parts,
        })
    }

    fn all(&self, slot: Slot) -> Vec<Part<'t>> {
        self.parts
            .iter()
            .filter(|p| p.slot == slot)
            .copied()
            .collect()
    }

    fn first(&self, slot: Slot) -> Option<Part<'t>> {
        self.parts.iter().find(|p| p.slot == slot).copied()
    }

    fn word(&self, slot: Slot, word: &str, src: &str) -> Option<Part<'t>> {
        self.parts
            .iter()
            .find(|p| p.slot == slot && text(&p.node, src).trim() == word)
            .copied()
    }

    fn kind(&self) -> &'t str {
        self.node.kind()
    }

    /// How the declaration starts, for messages: `part def`, `action`.
    fn keyword(&self, src: &str) -> String {
        let words: Vec<&str> = self
            .parts
            .iter()
            .filter(|p| p.slot == Slot::Keyword)
            .map(|p| text(&p.node, src))
            .collect();
        if words.is_empty() {
            "feature".into()
        } else {
            words.join(" ")
        }
    }

    /// Where a missing part goes, and the order key of text inserted
    /// there: a prefix modifier before the first part that follows it in
    /// canonical order, anything else after the last part that precedes
    /// it.
    fn insertion(&self, slot: Slot, after: Slot) -> (usize, Key) {
        if slot < Slot::Keyword {
            let next = self.parts.iter().find(|p| p.slot > slot);
            return match next {
                Some(p) => (p.start, (p.slot.rank() * 2 - 1, slot.rank())),
                None => (self.node.start_byte(), (0, slot.rank())),
            };
        }
        let prev = self
            .parts
            .iter()
            .rev()
            .find(|p| p.slot <= after && p.slot >= Slot::Keyword);
        match prev {
            Some(p) => (p.end, (p.slot.rank() * 2 + 1, slot.rank())),
            None => (self.node.start_byte(), (0, slot.rank())),
        }
    }
}

/// Sort key of an edit: edits at one position apply in this order.
type Key = (u16, u16);

fn own(slot: Slot) -> Key {
    (slot.rank() * 2, slot.rank())
}

// --- formatting ---------------------------------------------------------------------

/// A reference as SysML text: each segment of `A::B` or `a.b` bare when it
/// can be, else quoted; quoted segments are kept as written.
fn format_reference(reference: &str) -> Result<String, EditError> {
    let r = reference.trim();
    let bad = || err(format!("`{reference}` is not a valid reference"));
    let mut out = String::new();
    let mut seg = String::new();
    let flush = |seg: &mut String, out: &mut String| -> Result<(), EditError> {
        let s = seg.trim();
        if s.is_empty() {
            return Err(bad());
        }
        if s.starts_with('\'') {
            out.push_str(s);
        } else {
            out.push_str(&format_name(s));
        }
        seg.clear();
        Ok(())
    };
    let mut chars = r.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                seg.push(c);
                let mut closed = false;
                while let Some(q) = chars.next() {
                    seg.push(q);
                    if q == '\\' {
                        if let Some(n) = chars.next() {
                            seg.push(n);
                        }
                    } else if q == '\'' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err(bad());
                }
            }
            ':' if chars.peek() == Some(&':') => {
                chars.next();
                flush(&mut seg, &mut out)?;
                out.push_str("::");
            }
            '.' => {
                flush(&mut seg, &mut out)?;
                out.push('.');
            }
            c => seg.push(c),
        }
    }
    flush(&mut seg, &mut out)?;
    Ok(out)
}

fn references(list: &[String]) -> Result<String, EditError> {
    Ok(list
        .iter()
        .filter(|s| !s.trim().is_empty())
        .map(|s| format_reference(s))
        .collect::<Result<Vec<_>, _>>()?
        .join(", "))
}

/// `4`, `[0..1]`, ` 1..* ` -> `[4]`, `[0..1]`, `[1..*]`; `None` when empty.
fn format_multiplicity(m: &str) -> Option<String> {
    let t = m.trim();
    let inner = t
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(t)
        .trim();
    (!inner.is_empty()).then(|| format!("[{inner}]"))
}

/// The text of a doc comment: no `/*`, `*/` or leading ` * `.
fn doc_text(comment: &str) -> String {
    let inner = comment.strip_prefix("/*").unwrap_or(comment);
    let inner = inner.strip_suffix("*/").unwrap_or(inner);
    let lines: Vec<&str> = inner
        .lines()
        .enumerate()
        .map(|(i, l)| {
            let t = l.trim();
            if i == 0 {
                return t;
            }
            match t.strip_prefix('*') {
                Some(rest) => rest.trim(),
                None => t,
            }
        })
        .collect();
    let first = lines.iter().position(|l| !l.is_empty());
    let last = lines.iter().rposition(|l| !l.is_empty());
    match (first, last) {
        (Some(a), Some(b)) => lines[a..=b].join("\n"),
        _ => String::new(),
    }
}

/// Text that is empty after trimming counts as "remove".
fn present(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

// --- reading ---------------------------------------------------------------------------

/// The references a relationship names: `A, B` of `:> A, B`.
fn targets<'t>(node: &Node<'t>) -> Vec<Node<'t>> {
    children(node)
        .into_iter()
        .filter(|c| matches!(c.kind(), "qualified_name" | "feature_chain"))
        .collect()
}

fn targets_text(node: &Node, src: &str) -> Option<String> {
    let t = targets(node);
    Some(src[t.first()?.start_byte()..t.last()?.end_byte()].to_string())
}

/// The expression of a `value_assignment`.
fn value_expression<'t>(va: &Node<'t>) -> Option<Node<'t>> {
    children(va)
        .into_iter()
        .rev()
        .find(|c| c.is_named() && c.kind() != "definition_body" && !is_comment(c))
}

fn value_kind(va: &Node) -> ValueKind {
    let tokens: Vec<&str> = children(va)
        .iter()
        .filter(|c| !c.is_named())
        .map(|c| c.kind())
        .collect();
    if tokens.contains(&"default") {
        ValueKind::Default
    } else if tokens.contains(&":=") {
        ValueKind::Initial
    } else {
        ValueKind::Bind
    }
}

fn current_doc(node: &Node, src: &str) -> Option<String> {
    let doc = body_of(node).and_then(|b| child_of_kind(&b, "doc_comment"))?;
    child_of_kind(&doc, "block_comment").map(|c| doc_text(text(&c, src)))
}

fn current_short_name(node: &Node, src: &str) -> Option<String> {
    let sn = child_of_kind(node, "short_name")?;
    let q = child_of_kind(&sn, "quoted_name").or(sn.named_child(0))?;
    Some(unquote_name(text(&q, src).trim()).to_string())
}

fn qualified_name(node: &Node, src: &str) -> String {
    let mut segs = Vec::new();
    let mut cur = Some(*node);
    while let Some(n) = cur {
        if let Some((name, short)) = declared_names(&n, src) {
            segs.push(format_name(&name.or(short).unwrap_or_default()));
        }
        cur = n.parent();
    }
    segs.reverse();
    segs.join("::")
}

fn kind_name(node: &Node, src: &str) -> String {
    if let Some(k) = crate::parser::def_kind_from_node(node, src.as_bytes()) {
        if let Ok(serde_json::Value::String(s)) = serde_json::to_value(k) {
            return s;
        }
    }
    crate::parser::usage_kind_from_node(node, src.as_bytes())
        .unwrap_or("feature")
        .to_string()
}

fn details(h: &Header, src: &str) -> ElementDetails {
    let node = h.node;
    let typing = h.first(Slot::Typing);
    let tilde = typing.and_then(|p| child_of_kind(&p.node, "~"));
    let va = h.first(Slot::Value).map(|p| p.node);
    let value = va
        .as_ref()
        .and_then(value_expression)
        .map(|e| text(&e, src).to_string());
    ElementDetails {
        name: h
            .first(Slot::Name)
            .map(|p| text(&p.node, src).to_string())
            .unwrap_or_default(),
        short_name: current_short_name(&node, src),
        kind: kind_name(&node, src),
        is_definition: h.family != Family::Usage,
        ty: typing.and_then(|p| targets_text(&p.node, src)),
        conjugated: tilde.is_some(),
        specializes: h
            .all(Slot::Specialization)
            .iter()
            .flat_map(|p| targets(&p.node))
            .map(|t| text(&t, src).to_string())
            .collect(),
        redefines: h
            .first(Slot::Redefinition)
            .and_then(|p| targets_text(&p.node, src)),
        multiplicity: h
            .parts
            .iter()
            .find(|p| p.node.kind() == "multiplicity")
            .map(|p| {
                let close = child_of_kind(&p.node, "]").map_or(p.end, |c| c.end_byte());
                src[p.node.start_byte()..close].to_string()
            }),
        value_kind: value.as_ref().and(va.as_ref()).map(value_kind),
        value,
        direction: h
            .first(Slot::Direction)
            .and_then(|p| match text(&p.node, src).trim() {
                "in" => Some(Direction::In),
                "out" => Some(Direction::Out),
                "inout" => Some(Direction::Inout),
                _ => None,
            }),
        is_abstract: h.word(Slot::Abstract, "abstract", src).is_some(),
        readonly: h.word(Slot::Readonly, "readonly", src).is_some(),
        derived: h.word(Slot::Derived, "derived", src).is_some(),
        end: h.first(Slot::End).is_some(),
        doc: current_doc(&node, src),
        qualified_name: qualified_name(&node, src),
    }
}

/// The first declaration that starts on `line` (1-based), outermost first.
fn declaration_on_line<'t>(node: Node<'t>, src: &str, line: usize) -> Option<Node<'t>> {
    if node.start_position().row + 1 > line {
        return None;
    }
    if node.start_position().row + 1 == line
        && family(node.kind()).is_some()
        && declared_names(&node, src).is_some()
    {
        return Some(node);
    }
    if node.end_position().row + 1 < line {
        return None;
    }
    children(&node)
        .into_iter()
        .find_map(|c| declaration_on_line(c, src, line))
}

/// What the declaration of an element says. `element` (a simple or
/// qualified name) picks it, preferring the one on `line` (1-based);
/// without a name, the first declaration on `line` is read.
pub fn element_details(
    source: &str,
    line: usize,
    element: Option<&str>,
) -> Result<ElementDetails, EditError> {
    let tree = parse(source);
    let node = match element.map(str::trim).filter(|e| !e.is_empty()) {
        Some(e) => locate(&tree, source, e, line)?,
        None => declaration_on_line(tree.root_node(), source, line)
            .ok_or_else(|| err(format!("no element is declared on line {line}")))?,
    };
    Ok(details(&Header::read(node, source)?, source))
}

// --- editing ----------------------------------------------------------------------------

struct Keyed {
    edit: TextEdit,
    key: Key,
}

struct Editor<'a, 't> {
    src: &'a str,
    h: Header<'t>,
    out: Vec<Keyed>,
}

impl<'t> Editor<'_, 't> {
    fn push(&mut self, start: usize, end: usize, new_text: String, key: Key) {
        self.out.push(Keyed {
            edit: TextEdit {
                start_byte: start,
                end_byte: end,
                new_text,
            },
            key,
        });
    }

    fn replace(&mut self, start: usize, end: usize, new_text: String, slot: Slot) {
        self.push(start, end, new_text, own(slot));
    }

    fn remove(&mut self, part: &Part) {
        // A prefix keyword goes with the blank after it: `in item x` ->
        // `item x`.
        let rest = &self.src[part.end..];
        let blank = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        if part.slot < Slot::Keyword && blank > 0 {
            self.push(part.start, part.end + blank, String::new(), own(part.slot));
            return;
        }
        let e = removal(self.src, part.start, part.end);
        self.push(e.start_byte, e.end_byte, e.new_text, own(part.slot));
    }

    fn insert(&mut self, slot: Slot, after: Slot, new_text: String) {
        let (at, key) = self.h.insertion(slot, after);
        self.push(at, at, new_text, key);
    }

    fn unsupported(&self, field: &str) -> EditError {
        err(format!(
            "`{field}` cannot be set on a `{}` declaration",
            self.h.keyword(self.src)
        ))
    }

    fn require(&self, ok: bool, field: &str) -> Result<(), EditError> {
        if ok {
            Ok(())
        } else {
            Err(self.unsupported(field))
        }
    }

    fn is_usage(&self) -> bool {
        self.h.family == Family::Usage
    }

    fn set_type(
        &mut self,
        ty: &Option<Option<String>>,
        conj: Option<bool>,
    ) -> Result<(), EditError> {
        let typing = self.h.first(Slot::Typing);
        let new = ty.as_ref().map(present);
        match new {
            // Remove.
            Some(None) => {
                if conj == Some(true) {
                    return Err(err("`conjugated` needs a type"));
                }
                for p in self.h.all(Slot::Typing) {
                    self.remove(&p);
                }
                Ok(())
            }
            Some(Some(t)) => {
                self.require(self.is_usage(), "type")?;
                let f = format_reference(t)?;
                match typing {
                    Some(p) => {
                        let refs = targets(&p.node);
                        let (Some(first), Some(last)) = (refs.first(), refs.last()) else {
                            return Err(err("the declared type cannot be read"));
                        };
                        self.replace(first.start_byte(), last.end_byte(), f, Slot::Typing);
                        self.conjugate(&p, conj)
                    }
                    None => {
                        let tilde = if conj == Some(true) { "~" } else { "" };
                        self.insert(Slot::Typing, Slot::Typing, format!(" : {tilde}{f}"));
                        Ok(())
                    }
                }
            }
            None => match (typing, conj) {
                (_, None) => Ok(()),
                (Some(p), _) => self.conjugate(&p, conj),
                (None, Some(true)) => Err(err("`conjugated` needs a type")),
                (None, Some(false)) => Ok(()),
            },
        }
    }

    /// Add or remove `~` before an existing type.
    fn conjugate(&mut self, typing: &Part, conj: Option<bool>) -> Result<(), EditError> {
        let Some(want) = conj else { return Ok(()) };
        let tilde = child_of_kind(&typing.node, "~");
        let first = targets(&typing.node).first().map(|n| n.start_byte());
        match (want, tilde, first) {
            (true, None, Some(at)) => {
                if typing.node.kind() != "typed_by" {
                    return Err(err(
                        "a type given with `defined by` cannot be conjugated; use `: ~T`",
                    ));
                }
                // Before the type's own edit at the same position.
                self.push(at, at, "~".into(), (Slot::Typing.rank() * 2, 0));
            }
            (false, Some(t), Some(at)) => self.push(
                t.start_byte(),
                at,
                String::new(),
                (Slot::Typing.rank() * 2, 0),
            ),
            _ => {}
        }
        Ok(())
    }

    fn set_specializes(&mut self, list: &[String]) -> Result<(), EditError> {
        let parts = self.h.all(Slot::Specialization);
        let joined = references(list)?;
        if joined.is_empty() {
            for p in &parts {
                self.remove(p);
            }
            return Ok(());
        }
        self.require(self.h.family != Family::Package, "specializes")?;
        match parts.split_first() {
            Some((first, rest)) => {
                let refs = targets(&first.node);
                let (Some(a), Some(b)) = (refs.first(), refs.last()) else {
                    return Err(err("the specialization cannot be read"));
                };
                self.replace(a.start_byte(), b.end_byte(), joined, Slot::Specialization);
                for p in rest {
                    self.remove(p);
                }
            }
            None => self.insert(
                Slot::Specialization,
                Slot::Specialization,
                format!(" :> {joined}"),
            ),
        }
        Ok(())
    }

    fn set_redefines(&mut self, value: &Option<String>) -> Result<(), EditError> {
        let parts = self.h.all(Slot::Redefinition);
        let Some(v) = present(value) else {
            for p in &parts {
                self.remove(p);
            }
            return Ok(());
        };
        self.require(self.is_usage(), "redefines")?;
        let f = format_reference(v)?;
        match parts.split_first() {
            Some((first, rest)) => {
                let refs = targets(&first.node);
                let (Some(a), Some(b)) = (refs.first(), refs.last()) else {
                    return Err(err("the redefinition cannot be read"));
                };
                self.replace(a.start_byte(), b.end_byte(), f, Slot::Redefinition);
                for p in rest {
                    self.remove(p);
                }
            }
            None => self.insert(Slot::Redefinition, Slot::Redefinition, format!(" :>> {f}")),
        }
        Ok(())
    }

    fn set_multiplicity(&mut self, value: &Option<String>) -> Result<(), EditError> {
        let existing = self
            .h
            .parts
            .iter()
            .find(|p| p.node.kind() == "multiplicity")
            .copied();
        let Some(m) = value.as_deref().and_then(format_multiplicity) else {
            if let Some(p) = existing {
                self.remove(&p);
            }
            return Ok(());
        };
        self.require(self.is_usage(), "multiplicity")?;
        match existing {
            Some(p) => {
                let close = child_of_kind(&p.node, "]").map_or(p.end, |c| c.end_byte());
                self.replace(p.node.start_byte(), close, m, Slot::Multiplicity);
            }
            None => {
                // A feature without a keyword needs its type first
                // (`mass : Real[1]`).
                let after = if self.h.kind() == "feature_usage" {
                    Slot::Typing
                } else {
                    Slot::Multiplicity
                };
                let (at, key) = self.h.insertion(Slot::Multiplicity, after);
                let tight = self.h.parts.iter().any(|p| {
                    p.end == at && matches!(p.slot, Slot::Name | Slot::ShortName | Slot::Typing)
                });
                let pad = if tight { "" } else { " " };
                self.push(at, at, format!("{pad}{m}"), key);
            }
        }
        Ok(())
    }

    fn set_value(
        &mut self,
        value: &Option<Option<String>>,
        kind: Option<ValueKind>,
    ) -> Result<(), EditError> {
        let part = self.h.first(Slot::Value);
        let expr = part.as_ref().and_then(|p| value_expression(&p.node));
        let new = value.as_ref().map(present);
        match new {
            Some(None) => {
                if let Some(p) = part {
                    self.remove(&p);
                }
                return Ok(());
            }
            Some(Some(v)) => {
                self.require(VALUED.contains(&self.h.kind()), "value")?;
                match (part, expr) {
                    (Some(_), Some(e)) => {
                        self.replace(e.start_byte(), e.end_byte(), v.to_string(), Slot::Value)
                    }
                    (Some(_), None) => return Err(err("the value cannot be read")),
                    (None, _) => {
                        let op = kind.unwrap_or(ValueKind::Bind).operator();
                        self.insert(Slot::Value, Slot::Value, format!(" {op}{v}"));
                        return Ok(());
                    }
                }
            }
            None => {}
        }
        if let (Some(k), Some(p), Some(e)) = (kind, part, expr) {
            if value_kind(&p.node) != k {
                self.push(
                    p.node.start_byte(),
                    e.start_byte(),
                    k.operator().into(),
                    (Slot::Value.rank() * 2, 0),
                );
            }
        } else if kind.is_some() && part.is_none() && value.is_none() {
            return Err(err("`valueKind` needs a value"));
        }
        Ok(())
    }

    fn set_direction(&mut self, dir: &Option<Direction>) -> Result<(), EditError> {
        let parts = self.h.all(Slot::Direction);
        let Some(d) = dir else {
            for p in &parts {
                self.remove(p);
            }
            return Ok(());
        };
        self.require(DIRECTED.contains(&self.h.kind()), "direction")?;
        match parts.split_first() {
            Some((first, rest)) => {
                let (s, e) = (first.node.start_byte(), first.node.end_byte());
                self.replace(s, e, d.keyword().into(), Slot::Direction);
                for p in rest {
                    self.remove(p);
                }
            }
            None => self.insert(
                Slot::Direction,
                Slot::Direction,
                format!("{} ", d.keyword()),
            ),
        }
        Ok(())
    }

    /// Add or remove a prefix keyword (`abstract`, `readonly`, ...).
    fn set_flag(
        &mut self,
        slot: Slot,
        word: &str,
        want: bool,
        allowed: bool,
    ) -> Result<(), EditError> {
        let existing = if slot == Slot::End {
            self.h.first(Slot::End)
        } else {
            self.h.word(slot, word, self.src)
        };
        match (want, existing) {
            (true, None) => {
                self.require(allowed, word)?;
                self.insert(slot, slot, format!("{word} "));
            }
            (false, Some(p)) => self.remove(&p),
            _ => {}
        }
        Ok(())
    }

    fn may_be_end(&self) -> bool {
        match self.h.kind() {
            "end_feature" | "feature_usage" | "action_usage" => true,
            "usage" => self
                .h
                .parts
                .iter()
                .filter(|p| p.slot == Slot::Keyword)
                .all(|p| END_KEYWORDS.contains(&text(&p.node, self.src))),
            _ => false,
        }
    }

    fn set_short_name(&mut self, value: &Option<String>) -> Result<(), EditError> {
        let node = self.h.node;
        let want = present(value);
        if want.map(str::to_string) == current_short_name(&node, self.src) {
            return Ok(());
        }
        if want.is_some() {
            let ok = self.h.family != Family::Usage || SHORT_NAMED.contains(&self.h.kind());
            self.require(ok, "shortName")?;
        }
        for e in set_short_name(self.src, &node, want.unwrap_or(""))? {
            self.push(e.start_byte, e.end_byte, e.new_text, own(Slot::ShortName));
        }
        Ok(())
    }

    fn set_doc(&mut self, value: &Option<String>) -> Result<(), EditError> {
        let node = self.h.node;
        let want = present(value).map(|v| v.replace('\r', ""));
        if want == current_doc(&node, self.src) {
            return Ok(());
        }
        for e in set_doc(self.src, &node, want.as_deref().unwrap_or(""))? {
            self.push(e.start_byte, e.end_byte, e.new_text, own(Slot::Body));
        }
        Ok(())
    }

    /// The edits, each trimmed to the text that changes, with edits at
    /// one position combined in canonical order.
    fn finish(self) -> Result<EditPlan, EditError> {
        let src = self.src;
        let mut items: Vec<Keyed> = self
            .out
            .into_iter()
            .filter_map(|k| minimal(src, k.edit).map(|edit| Keyed { edit, key: k.key }))
            .collect();
        items.sort_by_key(|k| (k.edit.start_byte, k.key));
        let mut merged: Vec<TextEdit> = Vec::new();
        for Keyed { edit: e, .. } in items {
            if let Some(last) = merged.last_mut() {
                let same_start = e.start_byte == last.start_byte;
                let inside = e.start_byte < last.end_byte;
                if same_start && last.start_byte == last.end_byte {
                    // An insertion, then whatever starts there.
                    last.end_byte = e.end_byte;
                    last.new_text.push_str(&e.new_text);
                    continue;
                }
                if (same_start || inside) && e.start_byte == e.end_byte {
                    // An insertion inside (or at the start of) a change.
                    last.new_text.push_str(&e.new_text);
                    continue;
                }
                if inside {
                    if last.new_text.is_empty() && e.new_text.is_empty() {
                        last.end_byte = last.end_byte.max(e.end_byte);
                        continue;
                    }
                    return Err(err("these changes overlap; set fewer fields at once"));
                }
            }
            merged.push(e);
        }
        let mut plan = EditPlan::new();
        for e in merged {
            plan.add(e);
        }
        Ok(plan)
    }
}

/// `edit` without the text it leaves unchanged at either end; `None` if
/// it changes nothing.
fn minimal(src: &str, edit: TextEdit) -> Option<TextEdit> {
    let old = &src[edit.start_byte..edit.end_byte];
    let new = edit.new_text.as_str();
    let mut pre = 0;
    for ((i, a), b) in old.char_indices().zip(new.chars()) {
        if a != b {
            break;
        }
        pre = i + a.len_utf8();
    }
    let (old_r, new_r) = (&old[pre..], &new[pre..]);
    let mut suf = 0;
    for (a, b) in old_r.chars().rev().zip(new_r.chars().rev()) {
        if a != b {
            break;
        }
        suf += a.len_utf8();
    }
    let start = edit.start_byte + pre;
    let end = edit.end_byte - suf;
    let text = &new_r[..new_r.len() - suf];
    (start != end || !text.is_empty()).then(|| TextEdit {
        start_byte: start,
        end_byte: end,
        new_text: text.to_string(),
    })
}

/// Change the declaration of the element named `element` (simple or
/// qualified, as written; preferring the one declared on `line`, 1-based,
/// or 0 for any) as `set` says. Returns the minimal edits; an empty plan
/// when the declaration already says it all.
pub fn set_declaration(
    source: &str,
    element: &str,
    line: usize,
    set: &DeclarationSet,
) -> Result<EditPlan, EditError> {
    let tree = parse(source);
    let node = locate(&tree, source, element, line)?;
    let h = Header::read(node, source)?;
    let usage = h.family == Family::Usage;
    let kind = h.kind();
    let mut ed = Editor {
        src: source,
        h,
        out: Vec::new(),
    };
    ed.set_type(&set.ty, set.conjugated)?;
    if let Some(list) = &set.specializes {
        ed.set_specializes(list)?;
    }
    if let Some(r) = &set.redefines {
        ed.set_redefines(r)?;
    }
    if let Some(m) = &set.multiplicity {
        ed.set_multiplicity(m)?;
    }
    ed.set_value(&set.value, set.value_kind)?;
    if let Some(d) = &set.direction {
        ed.set_direction(d)?;
    }
    if let Some(b) = set.is_abstract {
        let ok = ed.h.family != Family::Package;
        ed.set_flag(Slot::Abstract, "abstract", b, ok)?;
    }
    let featured = usage && ["usage", "end_feature", "feature_usage"].contains(&kind);
    if let Some(b) = set.readonly {
        ed.set_flag(Slot::Readonly, "readonly", b, featured)?;
    }
    if let Some(b) = set.derived {
        ed.set_flag(Slot::Derived, "derived", b, featured)?;
    }
    if let Some(b) = set.end {
        let ok = ed.may_be_end();
        ed.set_flag(Slot::End, "end", b, ok)?;
    }
    if let Some(s) = &set.short_name {
        ed.set_short_name(s)?;
    }
    if let Some(d) = &set.doc {
        ed.set_doc(d)?;
    }
    ed.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::edit::apply_edits;
    use serde_json::json;

    fn set(value: serde_json::Value) -> DeclarationSet {
        serde_json::from_value(value).unwrap()
    }

    /// Apply `set` to `element` and check the result parses.
    fn edit(src: &str, element: &str, value: serde_json::Value) -> String {
        let p = set_declaration(src, element, 0, &set(value.clone()))
            .unwrap_or_else(|e| panic!("{element} {value}: {e}"));
        let out = apply_edits(src, &p).unwrap();
        let m = crate::parser::parse_file("t.sysml", &out);
        assert!(m.syntax_errors.is_empty(), "{out}\n{:?}", m.syntax_errors);
        // Setting it again changes nothing.
        let again = set_declaration(&out, element, 0, &set(value.clone())).unwrap();
        assert!(
            again.edits.is_empty(),
            "{element} {value} not idempotent on\n{out}\n{:?}",
            again.edits
        );
        out
    }

    fn fails(src: &str, element: &str, value: serde_json::Value) -> String {
        set_declaration(src, element, 0, &set(value))
            .map(|p| format!("{:?}", p.edits))
            .expect_err("should fail")
            .message
    }

    fn details_of(src: &str, element: &str) -> ElementDetails {
        element_details(src, 0, Some(element)).unwrap()
    }

    /// `(source, element, set, expected)`: each field set, changed and
    /// cleared on each kind of declaration.
    #[test]
    fn table() {
        let cases: &[(&str, &str, serde_json::Value, &str)] = &[
            // type
            ("part a;", "a", json!({"type": "A"}), "part a : A;"),
            ("part a : A;", "a", json!({"type": "B"}), "part a : B;"),
            ("part a : A;", "a", json!({"type": null}), "part a;"),
            ("part a : A[4];", "a", json!({"type": null}), "part a[4];"),
            (
                "attribute x;",
                "x",
                json!({"type": "ISQ::mass"}),
                "attribute x : ISQ::mass;",
            ),
            (
                "item i :> j;",
                "i",
                json!({"type": "I"}),
                "item i : I :> j;",
            ),
            ("action a;", "a", json!({"type": "Fly"}), "action a : Fly;"),
            ("state s;", "s", json!({"type": "S"}), "state s : S;"),
            (
                "requirement r;",
                "r",
                json!({"type": "R"}),
                "requirement r : R;",
            ),
            (
                "part p defined by Q;",
                "p",
                json!({"type": "R"}),
                "part p defined by R;",
            ),
            // conjugated ports
            (
                "port p;",
                "p",
                json!({"type": "P", "conjugated": true}),
                "port p : ~P;",
            ),
            (
                "port p : P;",
                "p",
                json!({"conjugated": true}),
                "port p : ~P;",
            ),
            (
                "port p : ~P;",
                "p",
                json!({"conjugated": false}),
                "port p : P;",
            ),
            ("port p : ~P;", "p", json!({"type": "Q"}), "port p : ~Q;"),
            ("port p : ~P;", "p", json!({"type": null}), "port p;"),
            // specializes
            (
                "part def A;",
                "A",
                json!({"specializes": ["B"]}),
                "part def A :> B;",
            ),
            (
                "part def A :> B;",
                "A",
                json!({"specializes": ["B", "C"]}),
                "part def A :> B, C;",
            ),
            (
                "part def A :> B, C {}",
                "A",
                json!({"specializes": ["C"]}),
                "part def A :> C {}",
            ),
            (
                "part def A :> B { }",
                "A",
                json!({"specializes": []}),
                "part def A { }",
            ),
            (
                "requirement def <'R'> D specializes X;",
                "D",
                json!({"specializes": ["Y"]}),
                "requirement def <'R'> D specializes Y;",
            ),
            (
                "state def S;",
                "S",
                json!({"specializes": ["T"]}),
                "state def S :> T;",
            ),
            (
                "action def A;",
                "A",
                json!({"specializes": ["B"]}),
                "action def A :> B;",
            ),
            (
                "enum def E :> F { enum a; }",
                "E",
                json!({"specializes": []}),
                "enum def E { enum a; }",
            ),
            (
                "part a : A;",
                "a",
                json!({"specializes": ["b"]}),
                "part a : A :> b;",
            ),
            (
                "part a : A[4] = x;",
                "a",
                json!({"specializes": ["b"]}),
                "part a : A[4] :> b = x;",
            ),
            (
                "part q : A subsets x;",
                "q",
                json!({"specializes": ["y"]}),
                "part q : A subsets y;",
            ),
            (
                "part a :> b :> c;",
                "a",
                json!({"specializes": ["d"]}),
                "part a :> d;",
            ),
            // redefines
            (
                "part a : A;",
                "a",
                json!({"redefines": "x"}),
                "part a : A :>> x;",
            ),
            (
                "part a :> b :>> x;",
                "a",
                json!({"redefines": "y"}),
                "part a :> b :>> y;",
            ),
            (
                "part a : A :>> x = 1;",
                "a",
                json!({"redefines": null}),
                "part a : A = 1;",
            ),
            (
                "attribute m redefines mass;",
                "m",
                json!({"redefines": "weight"}),
                "attribute m redefines weight;",
            ),
            (
                "part a :>> b.c;",
                "a",
                json!({"redefines": "b.d"}),
                "part a :>> b.d;",
            ),
            // multiplicity
            (
                "part a : A;",
                "a",
                json!({"multiplicity": "[4]"}),
                "part a[4] : A;",
            ),
            (
                "part a : A[4];",
                "a",
                json!({"multiplicity": "6"}),
                "part a : A[6];",
            ),
            (
                "part a : A[4];",
                "a",
                json!({"multiplicity": "[0..1]"}),
                "part a : A[0..1];",
            ),
            (
                "item i : I [0..1];",
                "i",
                json!({"multiplicity": null}),
                "item i : I;",
            ),
            (
                "attribute b[0..*] ordered : Real;",
                "b",
                json!({"multiplicity": "1..*"}),
                "attribute b[1..*] ordered : Real;",
            ),
            (
                "action a : A;",
                "a",
                json!({"multiplicity": "[2]"}),
                "action a[2] : A;",
            ),
            (
                "state s;",
                "s",
                json!({"multiplicity": "[1]"}),
                "state s[1];",
            ),
            (
                "requirement r : R;",
                "r",
                json!({"multiplicity": "[0..1]"}),
                "requirement r[0..1] : R;",
            ),
            (
                "part def P { in m : Real; }",
                "m",
                json!({"multiplicity": "[2]"}),
                "part def P { in m : Real[2]; }",
            ),
            // value
            (
                "attribute x : Real;",
                "x",
                json!({"value": "0.3"}),
                "attribute x : Real = 0.3;",
            ),
            (
                "attribute x : Real = 0.3;",
                "x",
                json!({"value": "0.5"}),
                "attribute x : Real = 0.5;",
            ),
            (
                "attribute x : Real = 0.3;",
                "x",
                json!({"value": null}),
                "attribute x : Real;",
            ),
            (
                "attribute x : Real = 0.3;",
                "x",
                json!({"valueKind": "default"}),
                "attribute x : Real default = 0.3;",
            ),
            (
                "attribute x : Real default = 2;",
                "x",
                json!({"value": "3", "valueKind": "bind"}),
                "attribute x : Real = 3;",
            ),
            (
                "attribute x : Real default 2;",
                "x",
                json!({"value": "4"}),
                "attribute x : Real default 4;",
            ),
            (
                "attribute x;",
                "x",
                json!({"value": "1", "valueKind": "initial"}),
                "attribute x := 1;",
            ),
            (
                "attribute x : Real = 1 { doc /* d */ }",
                "x",
                json!({"value": "2"}),
                "attribute x : Real = 2 { doc /* d */ }",
            ),
            (
                "attribute x : Real = 1 { }",
                "x",
                json!({"value": null}),
                "attribute x : Real { }",
            ),
            (
                "part def P { in m : Real; }",
                "m",
                json!({"value": "1"}),
                "part def P { in m : Real = 1; }",
            ),
            // direction
            (
                "part def P { item x; }",
                "x",
                json!({"direction": "in"}),
                "part def P { in item x; }",
            ),
            (
                "port def P { out item x : X; }",
                "x",
                json!({"direction": "in"}),
                "port def P { in item x : X; }",
            ),
            (
                "port def P { out item x : X; }",
                "x",
                json!({"direction": "inout"}),
                "port def P { inout item x : X; }",
            ),
            (
                "port def P { out item x : X; }",
                "x",
                json!({"direction": null}),
                "port def P { item x : X; }",
            ),
            (
                "part d { port p : ~P; }",
                "p",
                json!({"direction": "out"}),
                "part d { out port p : ~P; }",
            ),
            (
                "part def P { private item x; }",
                "x",
                json!({"direction": "out"}),
                "part def P { private out item x; }",
            ),
            (
                "part def P { in m : Real; }",
                "m",
                json!({"direction": "out"}),
                "part def P { out m : Real; }",
            ),
            (
                "action def A { in action a; }",
                "a",
                json!({"direction": null}),
                "action def A { action a; }",
            ),
            // abstract, readonly, derived, end
            (
                "part def A;",
                "A",
                json!({"abstract": true}),
                "abstract part def A;",
            ),
            (
                "abstract part def A;",
                "A",
                json!({"abstract": false}),
                "part def A;",
            ),
            (
                "private part def A;",
                "A",
                json!({"abstract": true}),
                "private abstract part def A;",
            ),
            (
                "private abstract part def A;",
                "A",
                json!({"abstract": false}),
                "private part def A;",
            ),
            (
                "part a : A;",
                "a",
                json!({"abstract": true}),
                "abstract part a : A;",
            ),
            (
                "in part a;",
                "a",
                json!({"abstract": true}),
                "in abstract part a;",
            ),
            (
                "attribute x : Real;",
                "x",
                json!({"readonly": true}),
                "readonly attribute x : Real;",
            ),
            (
                "readonly attribute x : Real;",
                "x",
                json!({"readonly": false}),
                "attribute x : Real;",
            ),
            (
                "attribute x : Real;",
                "x",
                json!({"derived": true}),
                "derived attribute x : Real;",
            ),
            (
                "in attribute x : Real;",
                "x",
                json!({"derived": true, "readonly": true}),
                "in readonly derived attribute x : Real;",
            ),
            (
                "derived ref attribute d : Real;",
                "d",
                json!({"derived": false}),
                "ref attribute d : Real;",
            ),
            (
                "part def C { part a : A; }",
                "a",
                json!({"end": true}),
                "part def C { end part a : A; }",
            ),
            (
                "part def C { end part a : A; }",
                "a",
                json!({"end": false}),
                "part def C { part a : A; }",
            ),
            (
                "part def C { end e : E; }",
                "e",
                json!({"end": false}),
                "part def C { e : E; }",
            ),
            // prefixes around tags, features without a keyword, layout
            (
                "#Tag part def A;",
                "A",
                json!({"abstract": true}),
                "abstract #Tag part def A;",
            ),
            (
                "part def P { in m : Real; }",
                "m",
                json!({"end": true}),
                "part def P { in end m : Real; }",
            ),
            (
                "part a\n    : A\n    :> b;",
                "a",
                json!({"type": null}),
                "part a\n    :> b;",
            ),
            (
                "part a\n    : A\n    :> b;",
                "a",
                json!({"type": "B", "redefines": "c"}),
                "part a\n    : B\n    :> b :>> c;",
            ),
            (
                "attribute 'µ' : Real = 1;",
                "µ",
                json!({"value": "2", "doc": "Mü."}),
                "attribute 'µ' : Real = 2 {\n    doc /* Mü. */\n}",
            ),
            // short name
            (
                "part def A;",
                "A",
                json!({"shortName": "A1"}),
                "part def <'A1'> A;",
            ),
            (
                "part def <'A1'> A;",
                "A",
                json!({"shortName": "A2"}),
                "part def <'A2'> A;",
            ),
            (
                "part def <'A1'> A;",
                "A",
                json!({"shortName": null}),
                "part def A;",
            ),
            (
                "part a : A;",
                "a",
                json!({"shortName": "x"}),
                "part <'x'> a : A;",
            ),
            (
                "requirement <'R1'> r : R;",
                "r",
                json!({"shortName": "R-2"}),
                "requirement <'R-2'> r : R;",
            ),
            (
                "package P;",
                "P",
                json!({"shortName": "p"}),
                "package <'p'> P;",
            ),
            (
                "package <'p'> P { }",
                "P",
                json!({"shortName": ""}),
                "package P { }",
            ),
            // doc
            (
                "part def A;",
                "A",
                json!({"doc": "An A."}),
                "part def A {\n    doc /* An A. */\n}",
            ),
            (
                "part def A { doc /* Old. */ }",
                "A",
                json!({"doc": "New."}),
                "part def A { doc /* New. */ }",
            ),
            (
                "part def A {\n    doc /* Old. */\n    part b;\n}",
                "A",
                json!({"doc": null}),
                "part def A {\n    part b;\n}",
            ),
            (
                "package P;",
                "P",
                json!({"doc": "Pkg."}),
                "package P {\n    doc /* Pkg. */\n}",
            ),
            (
                "package P { part def A; }",
                "P",
                json!({"doc": "Pkg."}),
                "package P { doc /* Pkg. */ part def A; }",
            ),
            (
                "attribute x : Real = 0.3;",
                "x",
                json!({"doc": "X."}),
                "attribute x : Real = 0.3 {\n    doc /* X. */\n}",
            ),
            (
                "action a : A;",
                "a",
                json!({"doc": "Do."}),
                "action a : A {\n    doc /* Do. */\n}",
            ),
            (
                "state s;",
                "s",
                json!({"doc": "S."}),
                "state s {\n    doc /* S. */\n}",
            ),
            // quoting
            (
                "part a;",
                "a",
                json!({"type": "My Type"}),
                "part a : 'My Type';",
            ),
            (
                "part a;",
                "a",
                json!({"type": "Lib::'Odd name'::T"}),
                "part a : Lib::'Odd name'::T;",
            ),
            (
                "part a;",
                "a",
                json!({"specializes": ["part", "b"]}),
                "part a :> 'part', b;",
            ),
            (
                "attribute 'max speed' : Real;",
                "'max speed'",
                json!({"value": "10"}),
                "attribute 'max speed' : Real = 10;",
            ),
            (
                "part def 'Big Part';",
                "Big Part",
                json!({"specializes": ["Small Part"]}),
                "part def 'Big Part' :> 'Small Part';",
            ),
        ];
        for (src, element, value, want) in cases {
            let out = edit(src, element, value.clone());
            assert_eq!(&out, want, "{src} + {value}");
        }
    }

    #[test]
    fn several_fields_at_once() {
        let out = edit(
            "part def D { part a; }",
            "a",
            json!({
                "type": "A", "multiplicity": "[4]", "specializes": ["b"],
                "redefines": "c", "value": "x", "direction": "in",
                "abstract": true, "shortName": "a1", "doc": "Four."
            }),
        );
        assert_eq!(
            out,
            "part def D { in abstract part <'a1'> a[4] : A :> b :>> c = x {\n    doc /* Four. */\n} }"
        );
        // Everything back off.
        let back = edit(
            &out,
            "a",
            json!({
                "type": null, "multiplicity": null, "specializes": [],
                "redefines": null, "value": null, "direction": null,
                "abstract": false, "shortName": null, "doc": null
            }),
        );
        assert_eq!(back, "part def D { part a {\n} }");
        let typed = edit(
            "port def P { attribute x : Real = 1; }",
            "x",
            json!({"type": "Integer", "multiplicity": "2", "value": "3", "valueKind": "default", "direction": "out", "readonly": true}),
        );
        assert_eq!(
            typed,
            "port def P { out readonly attribute x[2] : Integer default = 3; }"
        );
    }

    #[test]
    fn edits_are_minimal_and_keep_comments() {
        let src =
            "part def D {\n    // four of them\n    part rotors : Rotor[4]; // each a motor\n}\n";
        let p = set_declaration(src, "rotors", 3, &set(json!({"multiplicity": "6"}))).unwrap();
        assert_eq!(p.edits.len(), 1);
        let e = &p.edits[0];
        assert_eq!(&src[e.start_byte..e.end_byte], "4");
        assert_eq!(e.new_text, "6");
        let p = set_declaration(src, "rotors", 3, &set(json!({"type": "Rotor2"}))).unwrap();
        assert_eq!(
            (p.edits[0].start_byte, p.edits[0].new_text.as_str()),
            (src.find("[4]").unwrap(), "2")
        );
        let spaced = "part def\n    A /* a */ :> B ;";
        let out = edit(spaced, "A", json!({"specializes": ["C"], "abstract": true}));
        assert_eq!(out, "abstract part def\n    A /* a */ :> C ;");
    }

    #[test]
    fn same_values_change_nothing() {
        let src = "package P {\n    abstract part def <'D1'> D :> Base {\n        doc /* A drone.\n         * Two lines. */\n        in readonly attribute x : Real[1] default = 0.3;\n        port p : ~Q :>> q;\n    }\n}\n";
        for (element, value) in [
            (
                "D",
                json!({"specializes": ["Base"], "abstract": true, "shortName": "D1", "doc": "A drone.\nTwo lines."}),
            ),
            (
                "x",
                json!({"type": "Real", "multiplicity": "[1]", "value": "0.3", "valueKind": "default", "direction": "in", "readonly": true, "derived": false, "end": false}),
            ),
            (
                "p",
                json!({"type": "Q", "conjugated": true, "redefines": "q", "specializes": [], "value": null, "doc": null}),
            ),
        ] {
            let p = set_declaration(src, element, 0, &set(value.clone())).unwrap();
            assert!(p.edits.is_empty(), "{element} {value}: {:?}", p.edits);
        }
    }

    #[test]
    fn what_a_declaration_cannot_say() {
        assert!(fails("part def A;", "A", json!({"type": "T"}))
            .contains("`type` cannot be set on a `part def`"));
        assert!(fails("part def A;", "A", json!({"multiplicity": "2"})).contains("multiplicity"));
        assert!(fails("part def A;", "A", json!({"value": "2"})).contains("value"));
        assert!(fails("part def A;", "A", json!({"direction": "in"})).contains("direction"));
        assert!(fails("package P;", "P", json!({"specializes": ["Q"]})).contains("specializes"));
        assert!(fails("package P;", "P", json!({"abstract": true})).contains("abstract"));
        assert!(fails("state s;", "s", json!({"value": "1"})).contains("value"));
        assert!(fails("action a;", "a", json!({"shortName": "A1"})).contains("shortName"));
        assert!(fails("state s;", "s", json!({"end": true})).contains("end"));
        assert!(fails("part a;", "a", json!({"conjugated": true})).contains("needs a type"));
        assert!(
            fails("part p defined by Q;", "p", json!({"conjugated": true})).contains("defined by")
        );
        assert!(fails("part a;", "a", json!({"valueKind": "default"})).contains("needs a value"));
        assert!(fails("part a;", "b", json!({"type": "T"})).contains("not found"));
        // Clearing what a declaration cannot have is fine.
        let p = set_declaration("part def A;", "A", 0, &set(json!({"type": null, "multiplicity": null, "value": null, "direction": null, "readonly": false}))).unwrap();
        assert!(p.edits.is_empty());
        // Unknown fields are refused.
        assert!(serde_json::from_value::<DeclarationSet>(json!({"typ": "T"})).is_err());
    }

    #[test]
    fn line_picks_the_element() {
        let src = "part def A { part x : X; }\npart def B {\n    part x : X;\n}\n";
        let p = set_declaration(src, "x", 3, &set(json!({"type": "Y"}))).unwrap();
        let out = apply_edits(src, &p).unwrap();
        assert!(out.contains("part def A { part x : X; }"), "{out}");
        assert!(out.contains("    part x : Y;"), "{out}");
    }

    #[test]
    fn details_read_the_declaration() {
        let src = "package P {\n    abstract part def <'D1'> D :> Base, Other {\n        doc /* A drone.\n         * Two lines. */\n        in readonly derived attribute x : Real[1] ordered default = 0.3;\n        port p : ~Q :> r :>> q;\n        end part e : E;\n    }\n    package 'Sub Pkg' { use case def U; }\n}\n";
        let d = details_of(src, "D");
        assert_eq!(
            d,
            ElementDetails {
                name: "D".into(),
                short_name: Some("D1".into()),
                kind: "part".into(),
                is_definition: true,
                specializes: vec!["Base".into(), "Other".into()],
                is_abstract: true,
                doc: Some("A drone.\nTwo lines.".into()),
                qualified_name: "P::D".into(),
                ..Default::default()
            }
        );
        let x = details_of(src, "x");
        assert_eq!(x.ty.as_deref(), Some("Real"));
        assert_eq!(x.multiplicity.as_deref(), Some("[1]"));
        assert_eq!(x.value.as_deref(), Some("0.3"));
        assert_eq!(x.value_kind, Some(ValueKind::Default));
        assert_eq!(x.direction, Some(Direction::In));
        assert!(x.readonly && x.derived && !x.is_abstract && !x.end);
        assert_eq!((x.kind.as_str(), x.is_definition), ("attribute", false));
        assert_eq!(x.qualified_name, "P::D::x");
        let p = details_of(src, "p");
        assert_eq!((p.ty.as_deref(), p.conjugated), (Some("Q"), true));
        assert_eq!(p.specializes, vec!["r".to_string()]);
        assert_eq!(p.redefines.as_deref(), Some("q"));
        assert!(details_of(src, "e").end);
        let u = details_of(src, "U");
        assert_eq!(
            (u.kind.as_str(), u.qualified_name.as_str()),
            ("use_case", "P::'Sub Pkg'::U")
        );
        let pkg = element_details(src, 9, None).unwrap();
        assert_eq!(
            (pkg.name.as_str(), pkg.kind.as_str()),
            ("'Sub Pkg'", "package")
        );
        // By line alone: the outermost declaration on it.
        assert_eq!(element_details(src, 2, None).unwrap().name, "D");
        assert!(element_details(src, 3, None).is_err());
        let json = serde_json::to_value(&x).unwrap();
        assert_eq!(json["valueKind"], "default");
        assert_eq!(json["type"], "Real");
        assert_eq!(json["readonly"], true);
        assert!(json.get("abstract").is_none());
        assert_eq!(
            serde_json::to_value(&d).unwrap()["specializes"],
            json!(["Base", "Other"])
        );
    }

    #[test]
    fn details_round_trip_through_set() {
        let src = "package P {\n    part def D {\n        doc /* Line one\n         * line two. */\n        out port p : ~Q[2] :> r :>> s = t;\n    }\n}\n";
        for element in ["D", "p"] {
            let d = details_of(src, element);
            let mut v = serde_json::to_value(&d).unwrap();
            let o = v.as_object_mut().unwrap();
            for k in ["name", "kind", "isDefinition", "qualifiedName"] {
                o.remove(k);
            }
            if d.is_definition {
                o.remove("type");
            }
            let p = set_declaration(src, element, 0, &set(v.clone())).unwrap();
            assert!(p.edits.is_empty(), "{element} {v}: {:?}", p.edits);
        }
    }
}
