//! Table-cell edits: set one field of one element as a minimal text patch.
//!
//! A table row comes from an element (`sources[i]` of a rendered view:
//! its qualified name and line); a column from a field of it (see
//! [`crate::view_edit::FieldSpec`]). [`set_field`] writes a new value for
//! that field the way a person would edit the file:
//!
//! - an existing value is replaced in place, keeping the rest of the
//!   line (an enum keeps its qualifier, a string stays a string, a
//!   number keeps its unit);
//! - a missing value is added to the body — `:>> priority =
//!   Priority::low;`, `@StatusInfo { status = StatusKind::open; }` or a
//!   field inside an existing annotation — indented like its siblings;
//!   an element that ends with `;` gets a body;
//! - an empty value removes the redefinition, the field (and an
//!   annotation left empty) or the doc comment;
//! - `shortName` sets or replaces `<'ID'>`; `doc` replaces the comment
//!   text in the same comment style, or adds `doc /* ... */` as the
//!   first member; `name` renames the element and its references in the
//!   file (not in comments or strings).
//!
//! [`set_table_columns`] rewrites the `columns` of a view's
//! `@TableRendering`, adding an override to a view that has none.

use tree_sitter::{Node, Parser, Tree};

use super::edit::{EditError, EditPlan, TextEdit};
use crate::model::{unquote_name, Model};
use crate::view_edit::{
    self, classify_value_text, format_name, quote_name, string_literal, FieldSpec, FieldType,
    ValueKind,
};
use crate::view_render::split_path;

pub(super) fn err(message: impl Into<String>) -> EditError {
    EditError {
        message: message.into(),
    }
}

pub(super) fn parse(source: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&crate::parser::get_language())
        .expect("tree-sitter language");
    parser.parse(source, None).expect("parse")
}

pub(super) fn text<'s>(node: &Node, src: &'s str) -> &'s str {
    &src[node.start_byte()..node.end_byte()]
}

pub(super) fn children<'t>(node: &Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

pub(super) fn child_of_kind<'t>(node: &Node<'t>, kind: &str) -> Option<Node<'t>> {
    children(node).into_iter().find(|c| c.kind() == kind)
}

/// The last name of a reference: `A::'b c'` -> `b c`.
fn last_segment(reference: &str) -> String {
    split_path(reference).pop().unwrap_or_default()
}

// --- finding the element ----------------------------------------------------------

/// Nodes with a `name` field that are not named elements themselves.
const NOT_DECLARATIONS: [&str; 6] = [
    "assignment_statement",
    "doc_comment",
    "named_argument",
    "comment_element",
    "return_statement",
    "enum_member",
];

/// Name and short name (unquoted) of a declaration node.
pub(super) fn declared_names(node: &Node, src: &str) -> Option<(Option<String>, Option<String>)> {
    if NOT_DECLARATIONS.contains(&node.kind()) {
        return None;
    }
    // `end e : E;` names its end `end_name`.
    let name = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("end_name"))
        .map(|n| unquote_name(text(&n, src)).to_string());
    let short = child_of_kind(node, "short_name")
        .and_then(|s| child_of_kind(&s, "quoted_name").or(s.named_child(0)))
        .map(|q| unquote_name(text(&q, src)).to_string());
    if name.is_none() && short.is_none() {
        return None;
    }
    Some((name, short))
}

struct Candidate<'t> {
    node: Node<'t>,
    chain: Vec<String>,
}

fn collect_candidates<'t>(
    node: Node<'t>,
    src: &str,
    want: &str,
    chain: &mut Vec<String>,
    out: &mut Vec<Candidate<'t>>,
) {
    let names = declared_names(&node, src);
    let pushed = match &names {
        Some((name, short)) => {
            chain.push(name.clone().or(short.clone()).unwrap_or_default());
            if name.as_deref() == Some(want) || short.as_deref() == Some(want) {
                out.push(Candidate {
                    node,
                    chain: chain.clone(),
                });
            }
            true
        }
        None => false,
    };
    for c in children(&node) {
        collect_candidates(c, src, want, chain, out);
    }
    if pushed {
        chain.pop();
    }
}

/// The declaration of `element` (a simple or qualified name, as in a
/// table's `sources`), preferring the one on `line` (1-based; 0 for any).
pub(super) fn locate<'t>(
    tree: &'t Tree,
    src: &str,
    element: &str,
    line: usize,
) -> Result<Node<'t>, EditError> {
    let segs = split_path(element);
    let want = segs
        .last()
        .ok_or_else(|| err("no element given"))?
        .to_string();
    let mut found = Vec::new();
    collect_candidates(tree.root_node(), src, &want, &mut Vec::new(), &mut found);
    if line > 0 {
        if let Some(c) = found
            .iter()
            .find(|c| c.node.start_position().row + 1 == line)
        {
            return Ok(c.node);
        }
    }
    if let Some(c) = found.iter().find(|c| c.chain.ends_with(&segs)) {
        return Ok(c.node);
    }
    match found.as_slice() {
        [only] => Ok(only.node),
        [] => Err(err(format!("element `{element}` not found"))),
        _ => Err(err(format!(
            "element `{element}` is ambiguous here; give its line"
        ))),
    }
}

/// The type an element is declared with: `: T` for a usage, `:> T` for a
/// definition.
pub fn element_type(source: &str, element: &str, line: usize) -> Result<Option<String>, EditError> {
    let tree = parse(source);
    let node = locate(&tree, source, element, line)?;
    let ty = children(&node).into_iter().find_map(|c| match c.kind() {
        "typed_by" | "specialization" => children(&c)
            .into_iter()
            .find(|q| q.kind() == "qualified_name")
            .map(|q| text(&q, source).to_string()),
        _ => None,
    });
    Ok(ty)
}

pub(super) fn body_of<'t>(node: &Node<'t>) -> Option<Node<'t>> {
    children(node)
        .into_iter()
        .find(|c| c.kind().ends_with("_body") && c.kind() != "constraint_body")
}

/// Members of a body: its named children between the braces.
fn members<'t>(body: &Node<'t>) -> Vec<Node<'t>> {
    children(body)
        .into_iter()
        .filter(|c| c.is_named() && c.kind() != "hash_tag")
        .collect()
}

// --- layout -------------------------------------------------------------------

fn line_start(src: &str, pos: usize) -> usize {
    src[..pos].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(src: &str, pos: usize) -> usize {
    src[pos..].find('\n').map_or(src.len(), |i| pos + i)
}

/// Leading whitespace of the line containing `pos`.
fn line_indent(src: &str, pos: usize) -> &str {
    let start = line_start(src, pos);
    let rest = &src[start..];
    &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
}

fn starts_line(src: &str, pos: usize) -> bool {
    src[line_start(src, pos)..pos].trim().is_empty()
}

/// One level of indentation as the file writes it.
fn indent_unit(src: &str) -> String {
    for line in src.lines() {
        if line.starts_with('\t') {
            return "\t".into();
        }
        let n = line.len() - line.trim_start_matches(' ').len();
        let rest = line.trim_start();
        if n >= 2 && !rest.is_empty() && !rest.starts_with('*') {
            return " ".repeat(n.min(8));
        }
    }
    "    ".into()
}

/// Indentation for a member of `body`: that of its first member on its
/// own line, else one level deeper than the closing brace's line.
fn member_indent(src: &str, body: &Node) -> String {
    if let Some(m) = members(body)
        .into_iter()
        .find(|m| starts_line(src, m.start_byte()))
    {
        return line_indent(src, m.start_byte()).to_string();
    }
    let close = body.end_byte().saturating_sub(1);
    format!("{}{}", line_indent(src, close), indent_unit(src))
}

/// Indent every line after the first of `text` by `indent`.
fn indent_rest(text: &str, indent: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 || l.is_empty() {
                l.to_string()
            } else {
                format!("{indent}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Insert member `member` into `body`, after `after` or as the first member.
fn insert_member(src: &str, body: &Node, after: Option<&Node>, member: &str) -> TextEdit {
    let open = body.start_byte();
    let close = body.end_byte().saturating_sub(1);
    let multi_line = src[open..close].contains('\n');
    let (pos, new_text) = if multi_line {
        let indent = member_indent(src, body);
        let pos = after.map_or(open + 1, |n| n.end_byte());
        (pos, format!("\n{indent}{}", indent_rest(member, &indent)))
    } else {
        match after {
            Some(n) => (n.end_byte(), format!(" {member}")),
            None => {
                let tight = src[open + 1..].starts_with('}');
                (
                    open + 1,
                    format!(" {member}{}", if tight { " " } else { "" }),
                )
            }
        }
    };
    TextEdit {
        start_byte: pos,
        end_byte: pos,
        new_text,
    }
}

/// Give a `;`-terminated element a body holding `member`.
fn create_body(src: &str, element: &Node, member: &str) -> Result<TextEdit, EditError> {
    let semi = children(element)
        .into_iter()
        .rev()
        .find(|c| c.kind() == ";")
        .ok_or_else(|| err("the element has neither a body nor a closing `;`"))?;
    let indent = line_indent(src, element.start_byte()).to_string();
    let inner = format!("{indent}{}", indent_unit(src));
    Ok(TextEdit {
        start_byte: semi.start_byte(),
        end_byte: semi.end_byte(),
        new_text: format!(" {{\n{inner}{}\n{indent}}}", indent_rest(member, &inner)),
    })
}

/// Add `member` to `element`: into its body after `after` (or first), or
/// in a new body.
fn add_member(
    src: &str,
    element: &Node,
    after: Option<&Node>,
    member: &str,
) -> Result<TextEdit, EditError> {
    match body_of(element) {
        Some(body) => Ok(insert_member(src, &body, after, member)),
        None => create_body(src, element, member),
    }
}

/// Remove a node: its whole line(s) when it stands alone, else the node
/// and the blank before it.
pub(super) fn removal(src: &str, start: usize, end: usize) -> TextEdit {
    let ls = line_start(src, start);
    let le = line_end(src, end);
    if src[ls..start].trim().is_empty() && src[end..le].trim().is_empty() {
        let to = if le < src.len() { le + 1 } else { le };
        return TextEdit {
            start_byte: ls,
            end_byte: to,
            new_text: String::new(),
        };
    }
    if src[ls..start].trim().is_empty() {
        // First on a line that continues: take the blank after it.
        let after = src[end..le].len() - src[end..le].trim_start_matches([' ', '\t']).len();
        return TextEdit {
            start_byte: start,
            end_byte: end + after,
            new_text: String::new(),
        };
    }
    let before = src[..start].len() - src[..start].trim_end_matches([' ', '\t']).len();
    TextEdit {
        start_byte: start - before,
        end_byte: end,
        new_text: String::new(),
    }
}

fn replace(node: &Node, new_text: String) -> TextEdit {
    TextEdit {
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        new_text,
    }
}

fn plan(edits: Vec<TextEdit>) -> EditPlan {
    let mut p = EditPlan::new();
    for e in edits {
        p.add(e);
    }
    p
}

// --- values ---------------------------------------------------------------------

/// The feature a body member sets: `:>> f = v;`, `attribute :>> f = v;`,
/// `attribute redefines f = v;`, `attribute f = v;` or `f = v;`.
fn member_feature(node: &Node, src: &str) -> Option<String> {
    match node.kind() {
        "assignment_statement" => node
            .child_by_field_name("name")
            .map(|n| unquote_name(text(&n, src)).to_string()),
        "standalone_redefines" => {
            child_of_kind(node, "qualified_name").map(|q| last_segment(text(&q, src)))
        }
        "usage" => {
            for c in children(node) {
                match c.kind() {
                    "redefinition" => {
                        return child_of_kind(&c, "qualified_name")
                            .map(|q| last_segment(text(&q, src)))
                    }
                    "keyword_type_relationship" if text(&c, src).starts_with("redefines") => {
                        return child_of_kind(&c, "qualified_name")
                            .map(|q| last_segment(text(&q, src)))
                    }
                    _ => {}
                }
            }
            child_of_kind(node, "value_assignment")?;
            node.child_by_field_name("name")
                .map(|n| unquote_name(text(&n, src)).to_string())
        }
        _ => None,
    }
}

/// The expression after `=` in a member.
fn value_node<'t>(member: &Node<'t>) -> Option<Node<'t>> {
    let va = child_of_kind(member, "value_assignment")?;
    children(&va)
        .into_iter()
        .rev()
        .find(|c| c.is_named() && c.kind() != "definition_body")
}

/// How to write a value: the resolved type, refined by the value it
/// replaces (or, failing both, by how the file writes this field
/// elsewhere).
fn effective_type(
    resolved: Option<&FieldType>,
    existing: Option<&str>,
    file_hint: impl FnOnce() -> Option<FieldType>,
) -> FieldType {
    let from_existing = existing.map(|e| classify_value_text(&[], e));
    let mut ty = resolved.cloned().unwrap_or_default();
    if let Some(ex) = from_existing.filter(|t| t.kind.is_some()) {
        if ty.kind.is_none() {
            ty.kind = ex.kind;
        }
        if ex.enum_type.is_some() {
            ty.enum_type = ex.enum_type;
        }
    }
    if ty.kind.is_none() {
        if let Some(hint) = file_hint() {
            ty.kind = hint.kind;
            if ty.enum_type.is_none() {
                ty.enum_type = hint.enum_type;
            }
        }
    }
    ty
}

/// One enum literal as SysML text, checked against the known literals.
fn enum_literal(ty: &FieldType, value: &str) -> Result<String, EditError> {
    // A qualified value (`Priority::low`, as annotation rows show it)
    // names its literal last.
    let value = value.trim();
    let lit = value.rsplit("::").next().unwrap_or(value).trim();
    let written = if ty.literals.is_empty() {
        // After `::` a reserved word reads as a name (`Method::analysis`);
        // only a name with other characters needs quotes.
        let name = unquote_name(lit);
        if view_edit::is_plain_identifier(name) {
            name.to_string()
        } else {
            quote_name(name)
        }
    } else {
        ty.literals
            .iter()
            .find(|(d, w)| d == unquote_name(lit) || w == lit)
            .map(|(_, w)| w.clone())
            .ok_or_else(|| {
                let names: Vec<&str> = ty.literals.iter().map(|(d, _)| d.as_str()).collect();
                err(format!(
                    "`{lit}` is not one of {}{}",
                    ty.enum_type
                        .as_deref()
                        .map(|t| format!("{t}'s literals: "))
                        .unwrap_or_default(),
                    names.join(", ")
                ))
            })?
    };
    Ok(match ty.enum_type.as_deref() {
        Some(q) => format!("{q}::{written}"),
        None => written,
    })
}

/// A cell value as SysML text of the given type.
fn format_value(ty: &FieldType, value: &str) -> Result<String, EditError> {
    let v = value.trim();
    match ty.kind {
        Some(ValueKind::Enum) => enum_literal(ty, v),
        Some(ValueKind::EnumList) => {
            let items: Vec<String> = v
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| enum_literal(ty, s))
                .collect::<Result<_, _>>()?;
            Ok(match items.as_slice() {
                [one] => one.clone(),
                _ => format!("({})", items.join(", ")),
            })
        }
        Some(ValueKind::Number) => {
            let number = v.split('[').next().unwrap_or("").trim();
            if number.parse::<f64>().is_err() {
                return Err(err(format!("`{v}` is not a number")));
            }
            Ok(v.to_string())
        }
        Some(ValueKind::Text | ValueKind::LongText) => Ok(string_literal(v)),
        // Nothing says: a number or boolean as typed, anything else as
        // a string.
        None => {
            if v.parse::<f64>().is_ok() || v == "true" || v == "false" {
                Ok(v.to_string())
            } else {
                Ok(string_literal(v))
            }
        }
    }
}

/// Replace a value node, keeping a unit after a number (`3.5 [kg]`).
fn replace_value(node: &Node, ty: &FieldType, value: &str) -> Result<TextEdit, EditError> {
    if ty.kind == Some(ValueKind::Number) && node.kind() == "bracket_expression" {
        if let Some(num) = node.named_child(0).filter(|n| n.kind() == "number_literal") {
            if value.trim().parse::<f64>().is_ok() {
                return Ok(replace(&num, value.trim().to_string()));
            }
        }
    }
    Ok(replace(node, format_value(ty, value)?))
}

// --- set_field ------------------------------------------------------------------

/// Set field `field` of the element named `element` (on `line`, 1-based,
/// or 0) to `value`, a display string: an enum literal name, a
/// comma-separated list of them, a number or plain text. An empty value
/// clears the field.
///
/// `ty` is the field's type resolved against the project (see
/// [`resolve_field_type`]); without it the existing value, or the way the
/// file writes the field elsewhere, decides how the value is written.
pub fn set_field(
    source: &str,
    element: &str,
    line: usize,
    field: &FieldSpec,
    value: &str,
    ty: Option<&FieldType>,
) -> Result<EditPlan, EditError> {
    let tree = parse(source);
    if let FieldSpec::Metadata { ty: meta, field: f } = field {
        // A metadata row (`@Fmea about x { ... }`) points at the
        // annotation itself.
        if line > 0 {
            if let Some(ann) = annotations(tree.root_node(), source)
                .into_iter()
                .find(|a| a.ty == *meta && a.node.start_position().row + 1 == line)
            {
                return set_annotation_field(source, &ann, f, value, ty, meta).map(plan);
            }
        }
    }
    let node = locate(&tree, source, element, line)?;
    let edits = match field {
        FieldSpec::Name => return set_name(source, &tree, &node, value),
        FieldSpec::ShortName => set_short_name(source, &node, value)?,
        FieldSpec::Doc => set_doc(source, &node, value)?,
        FieldSpec::Feature(f) => set_feature(source, &node, f, value, ty)?,
        FieldSpec::Metadata { ty: meta, field: f } => {
            set_metadata(source, &tree, &node, meta, f, value, ty)?
        }
    };
    Ok(plan(edits))
}

/// The type of `field` on `element`, resolved against `models` (the
/// project): the element's declared type and its supertypes, else the
/// values the models already give that field.
pub fn resolve_field_type(
    models: &[Model],
    source: &str,
    element: &str,
    line: usize,
    field: &FieldSpec,
) -> Option<FieldType> {
    match field {
        FieldSpec::Name | FieldSpec::ShortName => Some(FieldType {
            kind: Some(ValueKind::Text),
            ..Default::default()
        }),
        FieldSpec::Doc => Some(FieldType {
            kind: Some(ValueKind::LongText),
            ..Default::default()
        }),
        FieldSpec::Feature(f) => element_type(source, element, line)
            .ok()
            .flatten()
            .and_then(|t| view_edit::feature_type(models, &t, f))
            .or_else(|| view_edit::feature_type_from_values(models, f)),
        FieldSpec::Metadata { ty, field } => view_edit::metadata_field_type(models, ty, field)
            .or_else(|| view_edit::metadata_type_from_values(models, ty, field)),
    }
}

fn set_name(source: &str, tree: &Tree, node: &Node, value: &str) -> Result<EditPlan, EditError> {
    let name = node
        .child_by_field_name("name")
        .ok_or_else(|| err("the element has no name to change"))?;
    let new_name = value.trim();
    if new_name.is_empty() {
        return Err(err("a name cannot be empty"));
    }
    let old = text(&name, source);
    let new = format_name(new_name);
    if unquote_name(old) == new_name {
        return Ok(EditPlan::new());
    }
    let model = crate::parser::parse_file("edit.sysml", source);
    let all = super::edit::rename_element(source, &model, old, &new)?;
    // Not inside comments or strings: those mention the name, they do
    // not refer to it.
    let mut prose = Vec::new();
    collect_prose(tree.root_node(), &mut prose);
    let mut p = EditPlan::new();
    for e in all.edits {
        if !prose
            .iter()
            .any(|(s, t)| *s <= e.start_byte && e.end_byte <= *t)
        {
            p.add(e);
        }
    }
    Ok(p)
}

fn collect_prose(node: Node, out: &mut Vec<(usize, usize)>) {
    match node.kind() {
        "block_comment" | "line_comment" | "comment" | "string_literal" => {
            out.push((node.start_byte(), node.end_byte()));
        }
        _ => {
            for c in children(&node) {
                collect_prose(c, out);
            }
        }
    }
}

pub(super) fn set_short_name(
    source: &str,
    node: &Node,
    value: &str,
) -> Result<Vec<TextEdit>, EditError> {
    let v = value.trim();
    let existing = child_of_kind(node, "short_name");
    Ok(match (existing, v.is_empty()) {
        (None, true) => Vec::new(),
        (Some(sn), true) => {
            // `<'ID'> name` -> `name`
            let after = source[sn.end_byte()..].len()
                - source[sn.end_byte()..]
                    .trim_start_matches([' ', '\t'])
                    .len();
            vec![TextEdit {
                start_byte: sn.start_byte(),
                end_byte: sn.end_byte() + after,
                new_text: String::new(),
            }]
        }
        (Some(sn), false) => {
            let q = child_of_kind(&sn, "quoted_name").or(sn.named_child(0));
            match q {
                Some(q) => {
                    let quoted = text(&q, source).starts_with('\'');
                    let new = if quoted {
                        quote_name(v)
                    } else {
                        format_name(v)
                    };
                    vec![replace(&q, new)]
                }
                None => vec![replace(&sn, format!("<{}>", quote_name(v)))],
            }
        }
        (None, false) => {
            let name = node
                .child_by_field_name("name")
                .ok_or_else(|| err("the element has no name to put a short name before"))?;
            vec![TextEdit {
                start_byte: name.start_byte(),
                end_byte: name.start_byte(),
                new_text: format!("<{}> ", quote_name(v)),
            }]
        }
    })
}

// --- doc ------------------------------------------------------------------------

/// The text for a comment: `*/` cannot appear inside one.
fn comment_safe(text: &str) -> String {
    text.trim().replace("*/", "* /").replace('\r', "")
}

/// Greedy word wrap of one paragraph line to `width` characters.
fn wrap(line: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in line.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    out.push(cur);
    out
}

/// A block comment holding `body`, laid out like `old` (the comment it
/// replaces) — or `/* ... */` on one line when there is none.
fn block_comment(src: &str, old: Option<&Node>, body: &str) -> String {
    let body = comment_safe(body);
    let old_text = old.map(|o| text(o, src));
    let multi_old = old_text.is_some_and(|t| t.contains('\n'));
    if !multi_old && !body.contains('\n') {
        return format!("/* {body} */");
    }
    // The prefix of continuation lines (` * `), whether text follows the
    // opening `/*`, whether the closing `*/` ends the last text line, and
    // how wide the old comment was.
    let (cont, inline_open, inline_close, width) = match (old, old_text.filter(|_| multi_old)) {
        (Some(o), Some(t)) => {
            let lines: Vec<&str> = t.lines().collect();
            let second = lines.get(1).copied().unwrap_or("");
            let cont = match second.find('*') {
                Some(i) if second[..i].trim().is_empty() => {
                    let after = &second[i + 1..];
                    let gap = if after.starts_with(' ') || after.is_empty() {
                        " "
                    } else {
                        ""
                    };
                    format!("{}*{gap}", &second[..i])
                }
                _ => format!("{} ", " ".repeat(column(src, o.start_byte()) + 1)),
            };
            let inline_open = !lines[0].trim_start_matches("/*").trim().is_empty();
            let last = lines.last().copied().unwrap_or("");
            let inline_close = !last
                .trim()
                .trim_end_matches("*/")
                .trim()
                .trim_start_matches('*')
                .trim()
                .is_empty();
            let start_col = column(src, o.start_byte());
            let width = lines
                .iter()
                .enumerate()
                .map(|(i, l)| l.chars().count() + if i == 0 { start_col } else { 0 })
                .max()
                .unwrap_or(0)
                .max(60);
            (cont, inline_open, inline_close, width)
        }
        _ => {
            let col = old.map_or(0, |o| column(src, o.start_byte()));
            (format!("{} * ", " ".repeat(col)), false, false, 100)
        }
    };
    let room = width.saturating_sub(cont.chars().count()).max(20);
    let lines: Vec<String> = body.lines().flat_map(|l| wrap(l.trim(), room)).collect();
    let mut out = String::from("/*");
    for (i, l) in lines.iter().enumerate() {
        if i == 0 && inline_open {
            out.push(' ');
        } else {
            out.push('\n');
            out.push_str(if l.is_empty() { cont.trim_end() } else { &cont });
        }
        out.push_str(l);
    }
    if inline_close {
        out.push_str(" */");
    } else {
        out.push('\n');
        out.push_str(cont.trim_end().trim_end_matches('*'));
        out.push_str("*/");
    }
    out
}

/// Character column of `pos` in its line.
fn column(src: &str, pos: usize) -> usize {
    src[line_start(src, pos)..pos].chars().count()
}

pub(super) fn set_doc(source: &str, node: &Node, value: &str) -> Result<Vec<TextEdit>, EditError> {
    let doc = body_of(node).and_then(|b| child_of_kind(&b, "doc_comment"));
    let blank = value.trim().is_empty();
    Ok(match (doc, blank) {
        (None, true) => Vec::new(),
        (Some(d), true) => vec![removal(source, d.start_byte(), d.end_byte())],
        (Some(d), false) => match child_of_kind(&d, "block_comment") {
            Some(c) => vec![replace(&c, block_comment(source, Some(&c), value))],
            None => vec![TextEdit {
                start_byte: d.end_byte(),
                end_byte: d.end_byte(),
                new_text: format!(" {}", block_comment(source, None, value)),
            }],
        },
        (None, false) => {
            // Continuation lines are indented with the member.
            let comment = block_comment(source, None, value);
            vec![add_member(source, node, None, &format!("doc {comment}"))?]
        }
    })
}

// --- features ---------------------------------------------------------------------

fn feature_members<'t>(body: &Node<'t>, src: &str) -> Vec<(Node<'t>, String)> {
    members(body)
        .into_iter()
        .filter_map(|m| member_feature(&m, src).map(|f| (m, f)))
        .collect()
}

fn set_feature(
    source: &str,
    node: &Node,
    feature: &str,
    value: &str,
    resolved: Option<&FieldType>,
) -> Result<Vec<TextEdit>, EditError> {
    let body = body_of(node);
    let existing = body.as_ref().and_then(|b| {
        feature_members(b, source)
            .into_iter()
            .find(|(_, f)| f == feature)
            .map(|(m, _)| m)
    });
    let clear = value.trim().is_empty();
    let hint = || {
        let m = crate::parser::parse_file("edit.sysml", source);
        view_edit::feature_type_from_values(std::slice::from_ref(&m), feature)
    };
    match existing {
        Some(member) => {
            let has_body = body_of(&member).is_some();
            match value_node(&member) {
                Some(v) if clear => {
                    if has_body {
                        // Keep the member's own body; drop ` = value`.
                        let va = child_of_kind(&member, "value_assignment").unwrap_or(v);
                        Ok(vec![removal(source, va.start_byte(), va.end_byte())])
                    } else {
                        Ok(vec![removal(
                            source,
                            member.start_byte(),
                            member.end_byte(),
                        )])
                    }
                }
                Some(v) => {
                    let ty = effective_type(resolved, Some(text(&v, source)), hint);
                    Ok(vec![replace_value(&v, &ty, value)?])
                }
                None if clear => {
                    if has_body {
                        Ok(Vec::new())
                    } else {
                        Ok(vec![removal(
                            source,
                            member.start_byte(),
                            member.end_byte(),
                        )])
                    }
                }
                None => {
                    // `:>> f;` or `:>> f { ... }`: add ` = value`.
                    let ty = effective_type(resolved, None, hint);
                    let at = children(&member)
                        .into_iter()
                        .find(|c| c.kind() == ";" || c.kind().ends_with("_body"))
                        .map_or(member.end_byte(), |c| c.start_byte());
                    let pad = if source[..at].ends_with(' ') { "" } else { " " };
                    let tail = if source[at..].starts_with('{') {
                        " "
                    } else {
                        ""
                    };
                    Ok(vec![TextEdit {
                        start_byte: at,
                        end_byte: at,
                        new_text: format!("{pad}= {}{tail}", format_value(&ty, value)?),
                    }])
                }
            }
        }
        None if clear => Ok(Vec::new()),
        None => {
            let ty = effective_type(resolved, None, hint);
            let member = format!(
                ":>> {} = {};",
                format_name(feature),
                format_value(&ty, value)?
            );
            let after = body.as_ref().and_then(|b| {
                let ms = members(b);
                ms.iter()
                    .rev()
                    .find(|m| member_feature(m, source).is_some())
                    .or_else(|| ms.iter().rev().find(|m| m.kind() == "doc_comment"))
                    .copied()
            });
            Ok(vec![add_member(source, node, after.as_ref(), &member)?])
        }
    }
}

// --- metadata ---------------------------------------------------------------------

/// A metadata annotation in the source.
struct Annotation<'t> {
    node: Node<'t>,
    /// Simple name of the metadata type.
    ty: String,
    /// The `about` target, as written.
    about: Option<String>,
    /// `{ ... }`, if any.
    body: Option<Node<'t>>,
    /// `@T;` — a bare reference with no body.
    bare: bool,
}

fn as_annotation<'t>(node: Node<'t>, src: &str) -> Option<Annotation<'t>> {
    match node.kind() {
        "metadata_annotation" | "metadata_usage" => {
            let mut ty = None;
            let mut about = None;
            let mut after_about = false;
            for c in children(&node) {
                match c.kind() {
                    "about" => after_about = true,
                    "typed_by" if ty.is_none() => {
                        ty =
                            child_of_kind(&c, "qualified_name").map(|q| last_segment(text(&q, src)))
                    }
                    "qualified_name" | "feature_chain" => {
                        if after_about {
                            about = Some(text(&c, src).to_string());
                        } else if ty.is_none() && node.kind() == "metadata_annotation" {
                            ty = Some(last_segment(text(&c, src)));
                        }
                    }
                    _ => {}
                }
            }
            Some(Annotation {
                body: child_of_kind(&node, "metadata_body"),
                node,
                ty: ty?,
                about,
                bare: false,
            })
        }
        "expression_statement" => {
            let access = child_of_kind(&node, "metadata_access_expression")?;
            let q = child_of_kind(&access, "qualified_name")?;
            Some(Annotation {
                node,
                ty: last_segment(text(&q, src)),
                about: None,
                body: None,
                bare: true,
            })
        }
        _ => None,
    }
}

/// Every annotation in the tree.
fn annotations<'t>(root: Node<'t>, src: &str) -> Vec<Annotation<'t>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if let Some(a) = as_annotation(n, src) {
            out.push(a);
        }
        let mut cs = children(&n);
        cs.reverse();
        stack.extend(cs);
    }
    out.sort_by_key(|a| a.node.start_byte());
    out
}

fn set_metadata(
    source: &str,
    tree: &Tree,
    node: &Node,
    meta: &str,
    field: &str,
    value: &str,
    resolved: Option<&FieldType>,
) -> Result<Vec<TextEdit>, EditError> {
    let body = body_of(node);
    // On the element: `@T { ... }` in its body ...
    let own = body.as_ref().and_then(|b| {
        members(b)
            .into_iter()
            .filter_map(|m| as_annotation(m, source))
            .find(|a| a.ty == meta && a.about.is_none())
    });
    // ... or `@T about element { ... }` elsewhere in the file.
    let names = declared_names(node, source).unwrap_or((None, None));
    let about = || {
        annotations(tree.root_node(), source).into_iter().find(|a| {
            a.ty == meta
                && a.about.as_deref().is_some_and(|t| {
                    let last = last_segment(t);
                    names.0.as_deref() == Some(&last) || names.1.as_deref() == Some(&last)
                })
        })
    };
    if let Some(ann) = own.or_else(about) {
        return set_annotation_field(source, &ann, field, value, resolved, meta);
    }
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    let ty = effective_type(resolved, None, || metadata_hint(source, meta, field));
    let text = format!(
        "@{meta} {{ {} = {}; }}",
        format_name(field),
        format_value(&ty, value)?
    );
    let after = body.as_ref().and_then(|b| {
        let ms = members(b);
        ms.iter()
            .rev()
            .find(|m| as_annotation(**m, source).is_some())
            .or_else(|| {
                ms.iter()
                    .rev()
                    .find(|m| member_feature(m, source).is_some())
            })
            .or(ms.last())
            .copied()
    });
    Ok(vec![add_member(source, node, after.as_ref(), &text)?])
}

fn metadata_hint(source: &str, meta: &str, field: &str) -> Option<FieldType> {
    let m = crate::parser::parse_file("edit.sysml", source);
    view_edit::metadata_type_from_values(std::slice::from_ref(&m), meta, field)
}

fn set_annotation_field(
    source: &str,
    ann: &Annotation,
    field: &str,
    value: &str,
    resolved: Option<&FieldType>,
    meta: &str,
) -> Result<Vec<TextEdit>, EditError> {
    let clear = value.trim().is_empty();
    let hint = || metadata_hint(source, meta, field);
    let Some(body) = ann.body else {
        if clear {
            return Ok(Vec::new());
        }
        let ty = effective_type(resolved, None, hint);
        let assignment = format!("{} = {};", format_name(field), format_value(&ty, value)?);
        if ann.bare {
            return Ok(vec![replace(
                &ann.node,
                format!("@{meta} {{ {assignment} }}"),
            )]);
        }
        let semi = children(&ann.node)
            .into_iter()
            .rev()
            .find(|c| c.kind() == ";")
            .ok_or_else(|| err("the annotation has neither a body nor a closing `;`"))?;
        return Ok(vec![replace(&semi, format!(" {{ {assignment} }}"))]);
    };
    let fields = feature_members(&body, source);
    match fields.iter().find(|(_, f)| f == field) {
        Some((member, _)) if clear => {
            if fields.len() == 1 && members(&body).len() == 1 {
                // Nothing left: drop the whole annotation.
                Ok(vec![removal(
                    source,
                    ann.node.start_byte(),
                    ann.node.end_byte(),
                )])
            } else {
                Ok(vec![removal(
                    source,
                    member.start_byte(),
                    member.end_byte(),
                )])
            }
        }
        Some((member, _)) => match value_node(member) {
            Some(v) => {
                let ty = effective_type(resolved, Some(text(&v, source)), hint);
                Ok(vec![replace_value(&v, &ty, value)?])
            }
            None => Err(err(format!("`{field}` has no value to replace"))),
        },
        None if clear => Ok(Vec::new()),
        None => {
            let ty = effective_type(resolved, None, hint);
            let assignment = format!("{} = {};", format_name(field), format_value(&ty, value)?);
            let last = members(&body).last().copied();
            Ok(vec![insert_member(
                source,
                &body,
                last.as_ref(),
                &assignment,
            )])
        }
    }
}

// --- table columns ------------------------------------------------------------------

/// Set the columns of view `view` (a `view def` or `view` usage): the
/// `columns` string of its `@TableRendering`, inserted as an override when
/// the view has no annotation of its own. Empty `columns` removes a
/// usage's override, so it shows its definition's columns again.
pub fn set_table_columns(
    source: &str,
    view: &str,
    columns: &[String],
) -> Result<EditPlan, EditError> {
    let tree = parse(source);
    let node = locate(&tree, source, view, 0)?;
    let is_view = children(&node).iter().any(|c| c.kind() == "view");
    if !is_view {
        return Err(err(format!("`{view}` is not a view")));
    }
    let is_def = children(&node).iter().any(|c| c.kind() == "def");
    let cols: Vec<&str> = columns
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .collect();
    if cols.is_empty() && is_def {
        return Err(err("a view def needs at least one column"));
    }
    let value = cols.join("; ");
    let text_type = FieldType {
        kind: Some(ValueKind::Text),
        ..Default::default()
    };
    let edits = set_metadata(
        source,
        &tree,
        &node,
        "TableRendering",
        "columns",
        &value,
        Some(&text_type),
    )?;
    Ok(plan(edits))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::edit::apply_edits;

    fn edit(src: &str, element: &str, line: usize, field: &str, value: &str) -> String {
        let spec = FieldSpec::parse(field).unwrap();
        let p = set_field(src, element, line, &spec, value, None)
            .unwrap_or_else(|e| panic!("{field}={value}: {e}"));
        let out = apply_edits(src, &p).unwrap();
        let m = crate::parser::parse_file("t.sysml", &out);
        assert!(m.syntax_errors.is_empty(), "{out}\n{:?}", m.syntax_errors);
        out
    }

    fn typed(
        src: &str,
        element: &str,
        field: &str,
        value: &str,
        ty: &FieldType,
    ) -> Result<String, EditError> {
        let spec = FieldSpec::parse(field).unwrap();
        let p = set_field(src, element, 0, &spec, value, Some(ty))?;
        Ok(apply_edits(src, &p).unwrap())
    }

    const REQ: &str = "package R {
    requirement <'SYS-001'> 'Flight endurance' : DroneRequirement {
        doc /* The drone shall fly for at least 25 minutes with a 1 kg payload in still air
             * before it reaches its landing reserve. */
        :>> kind = RequirementKind::performance;
        :>> priority = Priority::high;
        :>> verificationMethod = (VerificationMethod::analysis, VerificationMethod::test);
        @StatusInfo { status = StatusKind::done; owner = \"J. Hale\"; }
        require constraint { drone.endurance >= 25 [min] }
    }

    requirement <'SYS-002'> 'Payload' : DroneRequirement {
        doc /* Carry 1 kg. */
    }

    requirement bare : DroneRequirement;

    satisfy 'Flight endurance' by drone;
}
";

    fn priority_type() -> FieldType {
        FieldType {
            kind: Some(ValueKind::Enum),
            enum_type: Some("Priority".into()),
            literals: ["high", "medium", "low"]
                .iter()
                .map(|s| (s.to_string(), s.to_string()))
                .collect(),
        }
    }

    #[test]
    fn enum_values_are_replaced_in_place() {
        let out = edit(REQ, "R::'Flight endurance'", 2, "feature:priority", "low");
        assert!(
            out.contains("        :>> priority = Priority::low;\n"),
            "{out}"
        );
        assert_eq!(out.len(), REQ.len() + 3 - 4);
    }

    #[test]
    fn enum_values_are_checked_against_the_literals() {
        let e = typed(
            REQ,
            "'Flight endurance'",
            "feature:priority",
            "urgent",
            &priority_type(),
        )
        .unwrap_err();
        assert!(e.message.contains("high, medium, low"), "{e}");
    }

    #[test]
    fn missing_features_are_inserted_after_their_siblings() {
        let out = typed(
            REQ,
            "Payload",
            "feature:priority",
            "medium",
            &priority_type(),
        )
        .unwrap();
        assert!(
            out.contains(
                "        doc /* Carry 1 kg. */\n        :>> priority = Priority::medium;\n    }"
            ),
            "{out}"
        );
    }

    #[test]
    fn the_file_shows_how_to_write_a_missing_enum() {
        // No resolved type: another requirement writes `Priority::high`.
        let out = edit(REQ, "Payload", 0, "feature:priority", "low");
        assert!(out.contains(":>> priority = Priority::low;"), "{out}");
    }

    #[test]
    fn elements_without_a_body_get_one() {
        let out = typed(REQ, "bare", "feature:priority", "low", &priority_type()).unwrap();
        assert!(
            out.contains(
                "    requirement bare : DroneRequirement {\n        :>> priority = Priority::low;\n    }\n"
            ),
            "{out}"
        );
    }

    #[test]
    fn clearing_a_feature_removes_its_line() {
        let out = edit(REQ, "'Flight endurance'", 0, "feature:priority", "");
        assert!(!out.contains("priority"), "{out}");
        assert!(out.contains("RequirementKind::performance;\n        :>> verificationMethod"));
    }

    #[test]
    fn enum_lists() {
        let out = edit(
            REQ,
            "'Flight endurance'",
            0,
            "feature:verificationMethod",
            "test, inspection",
        );
        assert!(
            out.contains(
                ":>> verificationMethod = (VerificationMethod::test, VerificationMethod::inspection);"
            ),
            "{out}"
        );
        let one = edit(
            REQ,
            "'Flight endurance'",
            0,
            "feature:verificationMethod",
            "test",
        );
        assert!(
            one.contains(":>> verificationMethod = VerificationMethod::test;"),
            "{one}"
        );
        let list = FieldType {
            kind: Some(ValueKind::EnumList),
            enum_type: Some("VerificationMethod".into()),
            literals: vec![],
        };
        let new = typed(
            REQ,
            "Payload",
            "feature:verificationMethod",
            "test,analysis",
            &list,
        )
        .unwrap();
        assert!(
            new.contains(
                ":>> verificationMethod = (VerificationMethod::test, VerificationMethod::analysis);"
            ),
            "{new}"
        );
    }

    #[test]
    fn metadata_fields_are_replaced_and_kept_quoted() {
        let out = edit(
            REQ,
            "'Flight endurance'",
            2,
            "metadata:StatusInfo.status",
            "open",
        );
        assert!(
            out.contains("@StatusInfo { status = StatusKind::open; owner = \"J. Hale\"; }"),
            "{out}"
        );
        let owner = edit(
            REQ,
            "'Flight endurance'",
            2,
            "metadata:StatusInfo.owner",
            "A. \"Al\" Brandt",
        );
        assert!(
            owner.contains("owner = \"A. \\\"Al\\\" Brandt\"; }"),
            "{owner}"
        );
    }

    #[test]
    fn metadata_fields_and_annotations_are_inserted() {
        let out = edit(
            REQ,
            "'Flight endurance'",
            2,
            "metadata:StatusInfo.originator",
            "QA",
        );
        assert!(
            out.contains("owner = \"J. Hale\"; originator = \"QA\"; }"),
            "{out}"
        );
        let status = FieldType {
            kind: Some(ValueKind::Enum),
            enum_type: Some("StatusKind".into()),
            literals: vec![],
        };
        let added = typed(REQ, "Payload", "metadata:StatusInfo.status", "tbd", &status).unwrap();
        assert!(
            added.contains(
                "doc /* Carry 1 kg. */\n        @StatusInfo { status = StatusKind::tbd; }\n    }"
            ),
            "{added}"
        );
        // No type known: the file's other annotation shows the enum.
        let hinted = edit(REQ, "bare", 0, "metadata:StatusInfo.status", "open");
        assert!(
            hinted.contains(
                "requirement bare : DroneRequirement {\n        @StatusInfo { status = StatusKind::open; }\n    }"
            ),
            "{hinted}"
        );
    }

    #[test]
    fn clearing_metadata_fields() {
        let out = edit(
            REQ,
            "'Flight endurance'",
            2,
            "metadata:StatusInfo.owner",
            "",
        );
        assert!(
            out.contains("@StatusInfo { status = StatusKind::done; }"),
            "{out}"
        );
        let gone = edit(
            &out,
            "'Flight endurance'",
            2,
            "metadata:StatusInfo.status",
            "",
        );
        assert!(!gone.contains("@StatusInfo"), "{gone}");
        assert!(gone.contains("VerificationMethod::test);\n        require constraint"));
    }

    #[test]
    fn annotation_rows_edit_the_annotation_on_their_line() {
        let src = "package F {
    part battery;
    @Fmea about battery {
        failureMode = \"Thermal runaway\";
        severity = 8; likelihood = 3;
    }
}
";
        let out = edit(src, "F::battery", 3, "metadata:Fmea.severity", "9");
        assert!(out.contains("severity = 9; likelihood = 3;"), "{out}");
        let added = edit(src, "F::battery", 3, "metadata:Fmea.detection", "4");
        assert!(
            added.contains("likelihood = 3;\n        detection = 4;\n    }"),
            "{added}"
        );
        // Found through `about` from the element as well.
        let via = edit(src, "battery", 2, "metadata:Fmea.failureMode", "Swelling");
        assert!(via.contains("failureMode = \"Swelling\";"), "{via}");
    }

    #[test]
    fn short_names() {
        let out = edit(REQ, "'Flight endurance'", 0, "shortName", "SYS-100");
        assert!(
            out.contains("requirement <'SYS-100'> 'Flight endurance'"),
            "{out}"
        );
        let added = edit(REQ, "bare", 0, "shortName", "SYS-003");
        assert!(
            added.contains("requirement <'SYS-003'> bare : DroneRequirement;"),
            "{added}"
        );
        let cleared = edit(REQ, "Payload", 0, "shortName", "");
        assert!(
            cleared.contains("requirement 'Payload' : DroneRequirement"),
            "{cleared}"
        );
        // Located by its short name too.
        let by_id = edit(REQ, "SYS-002", 0, "shortName", "SYS-020");
        assert!(by_id.contains("<'SYS-020'> 'Payload'"), "{by_id}");
    }

    #[test]
    fn names_rename_references_in_the_file() {
        let out = edit(REQ, "'Flight endurance'", 2, "name", "Flight time");
        assert!(
            out.contains("requirement <'SYS-001'> 'Flight time'"),
            "{out}"
        );
        assert!(out.contains("satisfy 'Flight time' by drone;"), "{out}");
        let src = "part def Wheel { doc /* A Wheel. */ } part w : Wheel;";
        let plain = edit(src, "Wheel", 0, "name", "Tyre");
        assert_eq!(plain, "part def Tyre { doc /* A Wheel. */ } part w : Tyre;");
        let spec = FieldSpec::Name;
        assert!(set_field(src, "Wheel", 0, &spec, " ", None).is_err());
    }

    #[test]
    fn doc_keeps_the_comment_style() {
        let out = edit(
            REQ,
            "'Flight endurance'",
            2,
            "doc",
            "The drone shall fly for at least 30 minutes with a 1 kg payload in still air before it reaches its landing reserve.",
        );
        assert!(
            out.contains(
                "        doc /* The drone shall fly for at least 30 minutes with a 1 kg payload in still air\n             * before it reaches its landing reserve. */\n"
            ),
            "{out}"
        );
        let one = edit(REQ, "Payload", 0, "doc", "Carry 2 kg; no */ inside.");
        assert!(
            one.contains("doc /* Carry 2 kg; no * / inside. */"),
            "{one}"
        );
        let added = edit(REQ, "bare", 0, "doc", "A new one.");
        assert!(
            added.contains(
                "requirement bare : DroneRequirement {\n        doc /* A new one. */\n    }"
            ),
            "{added}"
        );
        let cleared = edit(REQ, "Payload", 0, "doc", "");
        assert!(
            cleared.contains("requirement <'SYS-002'> 'Payload' : DroneRequirement {\n    }"),
            "{cleared}"
        );
    }

    #[test]
    fn doc_block_style_and_first_member() {
        let src = "package P {\n    part def A {\n        doc\n        /*\n         * Old.\n         */\n        attribute x = 1;\n    }\n    part def B {\n        attribute y = 2;\n    }\n}\n";
        let out = edit(src, "A", 0, "doc", "New\nlines.");
        assert!(
            out.contains(
                "        doc\n        /*\n         * New\n         * lines.\n         */\n"
            ),
            "{out}"
        );
        let first = edit(src, "B", 0, "doc", "Bee.");
        assert!(
            first.contains("    part def B {\n        doc /* Bee. */\n        attribute y = 2;"),
            "{first}"
        );
    }

    #[test]
    fn numbers_keep_their_units_and_strings_stay_strings() {
        let src = "part def P {\n    attribute mass = 3.5 [kg];\n    attribute label = \"x\";\n    attribute n = 2;\n}\n";
        let out = edit(src, "P", 0, "feature:mass", "4.25");
        assert!(out.contains("attribute mass = 4.25 [kg];"), "{out}");
        let label = edit(src, "P", 0, "feature:label", "42");
        assert!(label.contains("attribute label = \"42\";"), "{label}");
        let n = edit(src, "P", 0, "feature:n", "7");
        assert!(n.contains("attribute n = 7;"), "{n}");
        let spec = FieldSpec::parse("feature:n").unwrap();
        assert!(set_field(src, "P", 0, &spec, "many", None).is_err());
        let qualified = edit(REQ, "Payload", 0, "feature:priority", "Priority::low");
        assert!(
            qualified.contains(":>> priority = Priority::low;"),
            "{qualified}"
        );
    }

    #[test]
    fn single_line_bodies() {
        let src = "part h : Hazard { :>> severity = 3; }\npart g : Hazard {}\n";
        let out = edit(src, "h", 0, "feature:description", "Hot");
        assert_eq!(
            out,
            "part h : Hazard { :>> severity = 3; :>> description = \"Hot\"; }\npart g : Hazard {}\n"
        );
        let empty = edit(src, "g", 0, "feature:severity", "2");
        assert!(
            empty.contains("part g : Hazard { :>> severity = 2; }"),
            "{empty}"
        );
        let cleared = edit(src, "h", 0, "feature:severity", "");
        assert!(cleared.starts_with("part h : Hazard { }"), "{cleared}");
    }

    #[test]
    fn other_redefinition_forms() {
        let src = "requirement def D {\n    attribute :>> priority = Priority::high;\n    attribute redefines kind = Kind::safety;\n}\n";
        let out = edit(src, "D", 0, "feature:priority", "low");
        assert!(
            out.contains("attribute :>> priority = Priority::low;"),
            "{out}"
        );
        let k = edit(src, "D", 0, "feature:kind", "functional");
        assert!(
            k.contains("attribute redefines kind = Kind::functional;"),
            "{k}"
        );
    }

    #[test]
    fn quoted_enum_literals() {
        let ty = FieldType {
            kind: Some(ValueKind::Enum),
            enum_type: Some("RiskCategory".into()),
            literals: vec![
                ("design".into(), "design".into()),
                ("use".into(), "'use'".into()),
            ],
        };
        let src = "part p { :>> category = RiskCategory::design; }";
        let out = typed(src, "p", "feature:category", "use", &ty).unwrap();
        assert_eq!(out, "part p { :>> category = RiskCategory::'use'; }");
    }

    #[test]
    fn bare_annotations_get_a_body() {
        let src = "requirement r {\n    @StatusInfo;\n}\n";
        let out = edit(src, "r", 0, "metadata:StatusInfo.owner", "Me");
        assert!(
            out.contains("    @StatusInfo { owner = \"Me\"; }\n"),
            "{out}"
        );
    }

    #[test]
    fn table_columns() {
        let src = "package V {\n    view def T {\n        @TableRendering {\n            rows = \"requirements\";\n            columns = \"id; title\";\n        }\n    }\n    view t : T {\n        expose V::*;\n    }\n    view u : T;\n}\n";
        let cols = |c: &[&str]| c.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let apply = |view: &str, c: &[&str]| {
            let p = set_table_columns(src, view, &cols(c)).unwrap();
            apply_edits(src, &p).unwrap()
        };
        let def = apply("T", &["title", "id", "status"]);
        assert!(def.contains("columns = \"title; id; status\";"), "{def}");
        let usage = apply("V::t", &["title"]);
        assert!(
            usage.contains(
                "        expose V::*;\n        @TableRendering { columns = \"title\"; }\n    }"
            ),
            "{usage}"
        );
        let short = apply("u", &["id"]);
        assert!(
            short
                .contains("    view u : T {\n        @TableRendering { columns = \"id\"; }\n    }"),
            "{short}"
        );
        // Replacing an override, then removing it.
        let again = set_table_columns(&usage, "t", &cols(&["id", "text"])).unwrap();
        let again = apply_edits(&usage, &again).unwrap();
        assert!(
            again.contains("@TableRendering { columns = \"id; text\"; }"),
            "{again}"
        );
        let reset = set_table_columns(&again, "t", &[]).unwrap();
        let reset = apply_edits(&again, &reset).unwrap();
        assert_eq!(reset, src);
        assert!(set_table_columns(src, "T", &[]).is_err());
        assert!(set_table_columns(src, "R", &cols(&["x"])).is_err());
    }

    #[test]
    fn element_types() {
        assert_eq!(
            element_type(REQ, "'Flight endurance'", 2)
                .unwrap()
                .as_deref(),
            Some("DroneRequirement")
        );
        assert_eq!(
            element_type("requirement def D :> Base;", "D", 0)
                .unwrap()
                .as_deref(),
            Some("Base")
        );
        assert!(element_type(REQ, "Nope", 0).is_err());
    }
}
