//! Code completion.
//!
//! [`complete`] is the project-wide, scope-aware completion used by the
//! browser editor; [`detect_context`] and [`completions`] are the
//! per-file model completion of the language server.

use std::collections::HashSet;

use serde::Serialize;

use super::index::{ElemId, ElemKind, Src, Workspace};
use super::lex::{self, TokKind};
use super::WorkspaceDef;
use crate::model::{DefKind, Model};
use crate::stdlib;

/// What a completion item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompletionKind {
    Keyword,
    Definition,
    Usage,
    Package,
    Library,
    Snippet,
}

/// Where a model completion came from — lets the language server pick
/// its item icons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOrigin {
    /// A definition (or package) of this kind.
    Def(DefKind),
    /// An owned member usage with this keyword.
    Member(String),
    /// A member inherited from a supertype.
    Inherited,
    Other,
}

/// One completion.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_snippet: bool,
    /// Ranking key; lower sorts first.
    #[serde(skip)]
    pub sort_text: Option<String>,
    #[serde(skip)]
    pub origin: ItemOrigin,
}

impl CompletionItem {
    fn new(label: impl Into<String>, kind: CompletionKind) -> Self {
        Self {
            label: label.into(),
            kind,
            detail: None,
            documentation: None,
            insert_text: None,
            is_snippet: false,
            sort_text: None,
            origin: ItemOrigin::Other,
        }
    }
}

// ---------------------------------------------------------------------
// Per-file completion (language server)
// ---------------------------------------------------------------------

/// What kind of completion context we're in.
#[derive(Debug, PartialEq)]
pub enum CompletionFilter {
    /// No special context — show everything.
    All,
    /// After `:` in a type position — show only definitions (type names).
    TypePosition,
    /// After `:>` in a specialization — show only definitions of matching kind.
    Specialization,
    /// After `.` — show members of the preceding element.
    MemberAccess(String),
    /// After `Pkg::` — show members of that (stdlib or workspace) package.
    QualifiedAccess(String),
}

fn trailing_name(prefix: &str) -> String {
    prefix
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

/// Determine completion context from the text before the cursor.
pub fn detect_context(source: &str, offset: usize) -> CompletionFilter {
    let before = &source[..offset.min(source.len())];
    let trimmed = before.trim_end();

    // Qualified access: "Pkg::" — scoped package members
    if let Some(prefix) = trimmed.strip_suffix("::") {
        let name = trailing_name(prefix);
        if !name.is_empty() {
            return CompletionFilter::QualifiedAccess(name);
        }
    }

    // Member access: "something."
    if let Some(prefix) = trimmed.strip_suffix('.') {
        let name = trailing_name(prefix);
        if !name.is_empty() {
            return CompletionFilter::MemberAccess(name);
        }
    }

    // Specialization: ":>" with optional whitespace
    if trimmed.ends_with(":>") || trimmed.ends_with(":> ") {
        return CompletionFilter::Specialization;
    }

    // Type position: ":" followed by optional whitespace, but not "::"
    // (qualified name), ":>" or ":=" (assignment)
    let last_non_ws = trimmed.as_bytes().last().copied();
    if last_non_ws == Some(b':') {
        let before_colon = &trimmed[..trimmed.len() - 1];
        let prev = before_colon.as_bytes().last().copied();
        if prev != Some(b':') && prev != Some(b'>') {
            return CompletionFilter::TypePosition;
        }
    }

    CompletionFilter::All
}

fn def_item(name: &str, kind: DefKind, detail: String, doc: Option<&String>) -> CompletionItem {
    CompletionItem {
        detail: Some(detail),
        documentation: doc.cloned(),
        origin: ItemOrigin::Def(kind),
        ..CompletionItem::new(
            name,
            if kind == DefKind::Package {
                CompletionKind::Package
            } else {
                CompletionKind::Definition
            },
        )
    }
}

/// Completion items from the current file, workspace defs, and stdlib.
/// `filter` restricts which items are returned based on cursor context.
pub fn completions(
    model: &Model,
    workspace_defs: &[WorkspaceDef],
    filter: &CompletionFilter,
) -> Vec<CompletionItem> {
    if let CompletionFilter::MemberAccess(ref parent) = filter {
        return member_completions(model, parent);
    }
    if let CompletionFilter::QualifiedAccess(ref pkg) = filter {
        return qualified_completions(model, workspace_defs, pkg);
    }
    let is_type_position = matches!(
        filter,
        CompletionFilter::TypePosition | CompletionFilter::Specialization
    );
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    // Current file definitions (in type position, skip packages)
    for def in &model.definitions {
        if is_type_position && def.kind == DefKind::Package {
            continue;
        }
        if seen.insert(def.name.clone()) {
            items.push(def_item(
                &def.name,
                def.kind,
                def.kind.label().to_string(),
                def.doc.as_ref(),
            ));
        }
    }

    // Workspace definitions (cross-file)
    for loc in workspace_defs {
        if is_type_position && loc.kind == DefKind::Package {
            continue;
        }
        if seen.insert(loc.name.clone()) {
            items.push(def_item(
                &loc.name,
                loc.kind,
                loc.kind.label().to_string(),
                loc.doc.as_ref(),
            ));
        }
    }

    // Standard library definitions — always available, ranked below
    // workspace items via sort_text so `attribute mass : ` offers Real,
    // MassValue, ISQ quantities, ... without drowning local types.
    let stdlib_defs = stdlib::parse_stdlib()
        .iter()
        .flat_map(|m| m.definitions.iter())
        // Package members that are usages in the stdlib source (KerML
        // `datatype Real`, `attribute mass`, ...) — exposed via the
        // enriched package index.
        .chain(stdlib::stdlib_package_defs().values().flatten());
    for def in stdlib_defs {
        if is_type_position && def.kind == DefKind::Package {
            continue;
        }
        if seen.insert(def.name.clone()) {
            let mut item = def_item(
                &def.name,
                def.kind,
                format!("{} (stdlib)", def.kind.label()),
                def.doc.as_ref(),
            );
            item.sort_text = Some(format!("1{}", def.name));
            items.push(item);
        }
    }

    // Rank workspace/file items above stdlib
    for item in &mut items {
        if item.sort_text.is_none() {
            item.sort_text = Some(format!("0{}", item.label));
        }
    }

    items
}

/// Members of `Pkg::` — stdlib package members plus workspace definitions
/// whose enclosing package matches.
fn qualified_completions(
    model: &Model,
    workspace_defs: &[WorkspaceDef],
    pkg: &str,
) -> Vec<CompletionItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    let detail = |kind: DefKind| format!("{} ({})", kind.label(), pkg);

    if let Some(defs) = stdlib::stdlib_package_defs().get(pkg) {
        for def in defs {
            if seen.insert(def.name.clone()) {
                items.push(def_item(
                    &def.name,
                    def.kind,
                    detail(def.kind),
                    def.doc.as_ref(),
                ));
            }
        }
    }

    // Definitions in the current file under a package with this name
    for def in &model.definitions {
        if def.parent_def.as_deref().map(crate::model::unquote_name) == Some(pkg)
            && seen.insert(def.name.clone())
        {
            items.push(def_item(&def.name, def.kind, detail(def.kind), None));
        }
    }

    // Workspace definitions whose qualified name lives under the package
    for loc in workspace_defs {
        if let Some(ref qn) = loc.qualified_name {
            let in_pkg = qn
                .rsplit_once("::")
                .map(|(prefix, _)| prefix == pkg || prefix.ends_with(&format!("::{}", pkg)))
                .unwrap_or(false);
            if in_pkg && seen.insert(loc.name.clone()) {
                items.push(def_item(&loc.name, loc.kind, detail(loc.kind), None));
            }
        }
    }

    items
}

/// Completions for members of a named element.
fn member_completions(model: &Model, parent_name: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    // `drone.` where drone is a usage: enumerate the members of its TYPE
    // (feature chains are written through usage names in practice).
    let parent_name: &str = model
        .usages
        .iter()
        .find(|u| u.name == parent_name)
        .and_then(|u| u.type_ref.as_deref())
        .map(crate::model::simple_name)
        .unwrap_or(parent_name);
    for usage in model.usages_in_def(parent_name) {
        items.push(CompletionItem {
            detail: usage.type_ref.clone(),
            origin: ItemOrigin::Member(usage.kind.clone()),
            ..CompletionItem::new(usage.name.clone(), CompletionKind::Usage)
        });
    }
    // Inherited members through the supertype
    if let Some(def) = model.find_def(parent_name) {
        if let Some(ref st) = def.super_type {
            let st_name = crate::model::simple_name(st);
            for usage in model.usages_in_def(st_name) {
                if !items.iter().any(|i| i.label == usage.name) {
                    items.push(CompletionItem {
                        detail: Some(format!(
                            "{} (inherited)",
                            usage.type_ref.as_deref().unwrap_or("")
                        )),
                        origin: ItemOrigin::Inherited,
                        ..CompletionItem::new(usage.name.clone(), CompletionKind::Usage)
                    });
                }
            }
        }
    }
    items
}

// ---------------------------------------------------------------------
// Project-wide completion (browser editor)
// ---------------------------------------------------------------------

/// Completions at a cursor: replace bytes `from..to` with an item.
#[derive(Debug, Clone, Serialize)]
pub struct Completions {
    pub from: usize,
    pub to: usize,
    pub items: Vec<CompletionItem>,
}

/// Keywords that start a member declaration or statement.
const STATEMENT_KEYWORDS: &[&str] = &[
    "abstract",
    "action",
    "actor",
    "alias",
    "allocate",
    "allocation",
    "analysis",
    "assert",
    "assume",
    "attribute",
    "bind",
    "calc",
    "comment",
    "concern",
    "connect",
    "connection",
    "constraint",
    "doc",
    "end",
    "entry",
    "enum",
    "exhibit",
    "exit",
    "expose",
    "first",
    "flow",
    "frame",
    "import",
    "in",
    "include",
    "individual",
    "inout",
    "interface",
    "item",
    "metadata",
    "objective",
    "occurrence",
    "out",
    "package",
    "part",
    "perform",
    "port",
    "private",
    "protected",
    "public",
    "ref",
    "rendering",
    "require",
    "requirement",
    "satisfy",
    "snapshot",
    "stakeholder",
    "state",
    "subject",
    "succession",
    "then",
    "timeslice",
    "transition",
    "use",
    "variant",
    "variation",
    "verification",
    "verify",
    "view",
    "viewpoint",
];

/// Keywords after which `def` makes a definition.
const DEF_KEYWORDS: &[&str] = &[
    "action",
    "allocation",
    "analysis",
    "attribute",
    "calc",
    "case",
    "concern",
    "connection",
    "constraint",
    "enum",
    "flow",
    "interface",
    "item",
    "metadata",
    "occurrence",
    "part",
    "port",
    "rendering",
    "requirement",
    "state",
    "verification",
    "view",
    "viewpoint",
];

/// Snippets offered at the start of a statement: (label, template, detail).
const SNIPPETS: &[(&str, &str, &str)] = &[
    ("package", "package ${1:Name} {\n\t$0\n}", "Package"),
    ("part def", "part def ${1:Name} {\n\t$0\n}", "Part definition"),
    ("part", "part ${1:name} : ${2:Type};", "Part usage"),
    ("attribute def", "attribute def ${1:Name} {\n\t$0\n}", "Attribute definition"),
    ("attribute", "attribute ${1:name} : ${2:Type};", "Attribute usage"),
    ("port def", "port def ${1:Name} {\n\t$0\n}", "Port definition"),
    ("port", "port ${1:name} : ${2:Type};", "Port usage"),
    ("item def", "item def ${1:Name} {\n\t$0\n}", "Item definition"),
    ("interface def", "interface def ${1:Name} {\n\tend ${2:source} : ${3:Port};\n\tend ${4:target} : ${5:Port};\n}", "Interface definition"),
    ("connection def", "connection def ${1:Name} {\n\tend ${2:source};\n\tend ${3:target};\n}", "Connection definition"),
    ("action def", "action def ${1:Name} {\n\t$0\n}", "Action definition"),
    ("action", "action ${1:name} : ${2:Type};", "Action usage"),
    ("state def", "state def ${1:Name} {\n\tentry; then ${2:initial};\n\tstate ${2:initial};\n\t$0\n}", "State machine"),
    ("transition", "transition ${1:name} first ${2:source} accept ${3:Trigger} then ${4:target};", "State transition"),
    ("requirement def", "requirement def ${1:Name} {\n\tdoc /* ${2:Description} */\n\tsubject ${3:subject} : ${4:Type};\n\t$0\n}", "Requirement definition"),
    ("requirement", "requirement ${1:name} : ${2:Type} {\n\tdoc /* ${3:Description} */\n}", "Requirement usage"),
    ("constraint def", "constraint def ${1:Name} {\n\t$0\n}", "Constraint definition"),
    ("calc def", "calc def ${1:Name} {\n\tin ${2:x} : ${3:Real};\n\treturn : ${4:Real};\n}", "Calculation definition"),
    ("enum def", "enum def ${1:Name} {\n\tenum ${2:first};\n\tenum ${3:second};\n}", "Enumeration definition"),
    ("use case def", "use case def ${1:Name} {\n\tsubject ${2:subject} : ${3:Type};\n\t$0\n}", "Use case definition"),
    ("verification def", "verification def ${1:Name} {\n\tsubject ${2:subject} : ${3:Type};\n\tobjective { verify ${4:requirement}; }\n}", "Verification case"),
    ("analysis def", "analysis def ${1:Name} {\n\tsubject ${2:subject} : ${3:Type};\n\treturn ${4:result} : ${5:Real};\n}", "Analysis case"),
    ("view def", "view def ${1:Name} {\n\t$0\n}", "View definition"),
    ("connect", "connect ${1:source} to ${2:target};", "Connection"),
    ("flow", "flow of ${1:Item} from ${2:source} to ${3:target};", "Flow"),
    ("satisfy", "satisfy ${1:requirement} by ${2:element};", "Satisfaction"),
    ("verify", "verify ${1:requirement};", "Verification"),
    ("import", "private import ${1:Package}::*;", "Import"),
    ("doc", "doc /* ${1:Description} */", "Documentation"),
];

/// Completion context derived from the tokens before the word.
enum Context {
    /// `Pkg::` or `x.` — members of the resolved prefix.
    Members { prefix_tok: usize, in_import: bool },
    /// After `import` — packages.
    Import,
    /// A type position: `:` (definitions) or `:>` (definitions and
    /// features).
    Types { features: bool },
    /// After `:>>` — inherited features to redefine.
    Redefine,
    /// After `part`, `port`, ... — `def`.
    AfterKindKeyword,
    /// Start of a statement — keywords and snippets.
    Statement,
    /// A declared name is being typed — nothing to offer.
    Nothing,
    /// Anything else: names in scope.
    Names,
}

/// Completions at byte `offset` of file `src`.
pub fn complete(ws: &Workspace, src: Src, offset: usize) -> Completions {
    let file = ws.file(src);
    let text = &file.text;
    let offset = offset.min(text.len());
    let empty = Completions {
        from: offset,
        to: offset,
        items: Vec::new(),
    };
    if file.lexed.comment_at(offset).is_some() {
        return empty;
    }
    let tokens = &file.lexed.tokens;
    // The word being completed: a name or keyword token touching the cursor.
    let mut word_tok = None;
    let i = file.lexed.token_after(offset);
    for j in [i.checked_sub(1), Some(i)].into_iter().flatten() {
        if let Some(t) = tokens.get(j) {
            if t.start < offset && offset <= t.end {
                match t.kind {
                    TokKind::Ident | TokKind::Keyword | TokKind::Quoted { .. } => {
                        word_tok = Some(j)
                    }
                    TokKind::Str | TokKind::Number => return empty,
                    TokKind::Punct => {}
                }
            }
        }
    }
    let (from, prev) = match word_tok {
        Some(j) => (tokens[j].start, j.checked_sub(1)),
        None => (
            offset,
            tokens.partition_point(|t| t.end <= offset).checked_sub(1),
        ),
    };
    let prefix = lex::unquote(&text[from..offset]);

    let context = context(ws, src, prev, from);
    // Elements are filtered by name before their items are built: the
    // type position alone offers a couple of thousand library types.
    let items_for = |ids: Vec<ElemId>| -> Vec<CompletionItem> {
        ids.into_iter()
            .filter(|&id| matches_prefix(&ws.elem(id).key, &prefix))
            .map(|id| element_item(ws, id))
            .collect()
    };
    let mut items = match context {
        Context::Nothing => Vec::new(),
        Context::Members {
            prefix_tok,
            in_import,
        } => {
            let path = ws.path_at(src, prefix_tok);
            let ns = ws.resolve_segment(src, &path, path.cursor);
            let mut items = items_for(ns.map(|ns| ws.members_of(ns)).unwrap_or_default());
            if in_import {
                let mut all = CompletionItem::new("*", CompletionKind::Keyword);
                all.detail = Some("all members".into());
                let mut rec = CompletionItem::new("**", CompletionKind::Keyword);
                rec.detail = Some("all members, recursively".into());
                items.insert(0, all);
                items.insert(1, rec);
            }
            items
        }
        Context::Import => package_items(ws),
        Context::Types { features } => items_for(type_ids(ws, features)),
        Context::Redefine => {
            let owner = ws.scope_at(src, from);
            let inherited = owner
                .map(|o| ws.types_of(o))
                .unwrap_or_default()
                .into_iter()
                .flat_map(|t| ws.members_of(t))
                .filter(|&m| !ws.elem(m).kind.is_definition())
                .collect();
            items_for(inherited)
        }
        Context::AfterKindKeyword => {
            let mut def = CompletionItem::new("def", CompletionKind::Keyword);
            def.detail = Some("definition".into());
            vec![def]
        }
        Context::Statement => {
            let mut items = keyword_items();
            items.extend(snippet_items());
            items
        }
        Context::Names => items_for(ws.visible(src, ws.scope_at(src, from))),
    };

    let mut seen = HashSet::new();
    items.retain(|item| matches_prefix(&item.label, &prefix) && seen.insert(item.label.clone()));
    Completions {
        from,
        to: offset,
        items,
    }
}

fn context(ws: &Workspace, src: Src, prev: Option<usize>, from: usize) -> Context {
    let file = ws.file(src);
    let text = &file.text;
    let tokens = &file.lexed.tokens;
    let Some(p) = prev else {
        return Context::Statement;
    };
    let pt = tokens[p];
    let ps = pt.text(text);
    let in_import = file.import_at(pt.start).is_some()
        || tokens[..=p]
            .iter()
            .rev()
            .take_while(|t| !matches!(t.text(text), ";" | "{" | "}"))
            .any(|t| t.is_keyword(text, "import"));
    if (ps == "::" || ps == ".") && p > 0 && tokens[p - 1].is_name() {
        return Context::Members {
            prefix_tok: p - 1,
            in_import,
        };
    }
    if pt.is_keyword(text, "import") {
        return Context::Import;
    }
    if pt.kind == TokKind::Punct {
        return match ps {
            "{" | "}" | ";" => Context::Statement,
            ":" | "~" => Context::Types { features: false },
            ":>" | "::>" => Context::Types {
                features: is_usage_header(ws, src, from),
            },
            ":>>" => Context::Redefine,
            "," => {
                // `:> A, B` continues a type list.
                let in_types = tokens[..p]
                    .iter()
                    .rev()
                    .take_while(|t| !matches!(t.text(text), ";" | "{" | "}" | "="))
                    .any(|t| matches!(t.text(text), ":" | ":>"));
                if in_types {
                    Context::Types { features: false }
                } else {
                    Context::Names
                }
            }
            _ => Context::Names,
        };
    }
    if pt.kind == TokKind::Keyword {
        if matches!(ps, "specializes" | "subsets" | "references")
            || (ps == "by" && p > 0 && tokens[p - 1].is_keyword(text, "defined"))
        {
            return Context::Types {
                features: ps != "specializes",
            };
        }
        if ps == "redefines" {
            return Context::Redefine;
        }
        if ps == "def" {
            return Context::Nothing;
        }
        if ps == "doc"
            && file
                .lexed
                .comments
                .iter()
                .any(|c| c.start >= pt.end && c.end <= from)
        {
            // `doc /* ... */` is a complete statement.
            return Context::Statement;
        }
        if DEF_KEYWORDS.contains(&ps) {
            // `part ` starts a usage name — or `def`. `connection` etc. in
            // a relationship (`flow of`, `then`) take names instead.
            let starts_statement = p == 0
                || matches!(tokens[p - 1].text(text), "{" | "}" | ";")
                || matches!(
                    tokens[p - 1].text(text),
                    "abstract"
                        | "variation"
                        | "private"
                        | "public"
                        | "protected"
                        | "in"
                        | "out"
                        | "inout"
                        | "ref"
                        | "individual"
                );
            if starts_statement {
                return Context::AfterKindKeyword;
            }
        }
        if matches!(
            ps,
            "abstract" | "variation" | "private" | "public" | "protected" | "standard" | "library"
        ) {
            return Context::Statement;
        }
        return Context::Names;
    }
    if pt.is_name() {
        // `part battery |` — after a declared name: nothing useful.
        return Context::Nothing;
    }
    Context::Names
}

/// True when `offset` is in the header of a usage (as opposed to a
/// definition), where `:>` subsets a feature.
fn is_usage_header(ws: &Workspace, src: Src, offset: usize) -> bool {
    let file = ws.file(src);
    file.innermost(offset)
        .map(|e| &file.elements[e as usize])
        .filter(|e| offset < e.header_end)
        .is_some_and(|e| !e.kind.is_definition())
}

/// Case-insensitive subsequence match, a superset of editor fuzzy
/// matching, so the editor can rank and filter the rest.
fn matches_prefix(label: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    let label = lex::unquote(label).to_lowercase();
    let mut chars = label.chars();
    prefix
        .to_lowercase()
        .chars()
        .all(|p| chars.by_ref().any(|c| c == p))
}

fn keyword_items() -> Vec<CompletionItem> {
    STATEMENT_KEYWORDS
        .iter()
        .map(|k| CompletionItem::new(*k, CompletionKind::Keyword))
        .collect()
}

fn snippet_items() -> Vec<CompletionItem> {
    SNIPPETS
        .iter()
        .map(|(label, template, detail)| CompletionItem {
            detail: Some((*detail).into()),
            insert_text: Some((*template).into()),
            is_snippet: true,
            ..CompletionItem::new(*label, CompletionKind::Snippet)
        })
        .collect()
}

/// Short documentation: the first paragraph, at most ~300 characters.
pub(crate) fn short_doc(doc: &str) -> String {
    let para = doc.split("\n\n").next().unwrap_or(doc).trim();
    if para.chars().count() <= 300 {
        return para.to_owned();
    }
    let cut: String = para.chars().take(300).collect();
    format!("{}…", cut.trim_end())
}

/// A completion for an element.
pub fn element_item(ws: &Workspace, id: ElemId) -> CompletionItem {
    let e = ws.elem(id);
    let library = matches!(id.src, Src::Library(_));
    let kind = if e.kind.is_package() {
        CompletionKind::Package
    } else if library {
        CompletionKind::Library
    } else if e.kind.is_definition() {
        CompletionKind::Definition
    } else {
        CompletionKind::Usage
    };
    let mut detail = e.kind.label();
    if !e.kind.is_definition() {
        if let Some(t) = e.types.first() {
            detail = format!("{detail} : {t}");
        }
    }
    let owner = ws
        .parent(id)
        .map(|p| ws.qualified_name(p))
        .filter(|q| !q.is_empty());
    if let Some(owner) = owner {
        detail = format!("{detail} · {owner}");
    }
    let doc = e.doc.clone().or_else(|| {
        // A usage without a doc: describe it by its type.
        if e.kind.is_definition() {
            return None;
        }
        ws.types_of(id)
            .first()
            .and_then(|&t| ws.elem(t).doc.clone())
    });
    CompletionItem {
        detail: Some(detail),
        documentation: doc.as_deref().map(short_doc),
        origin: match &e.kind {
            ElemKind::Def(k) => ItemOrigin::Def(*k),
            ElemKind::Usage(k) => ItemOrigin::Member(k.clone()),
            ElemKind::EnumLiteral => ItemOrigin::Other,
        },
        ..CompletionItem::new(lex::quote_name(&e.key), kind)
    }
}

/// Packages to import: the project's (qualified) and the library's.
fn package_items(ws: &Workspace) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let mut push = |ws: &Workspace, id: ElemId| {
        let qn = ws.qualified_name(id);
        let mut item = element_item(ws, id);
        item.label = qn.clone();
        let mut all = item.clone();
        all.label = format!("{qn}::*");
        all.detail = Some(format!("all members of {qn}"));
        items.push(item);
        items.push(all);
    };
    for (f, file) in ws.files.iter().enumerate() {
        for (i, e) in file.elements.iter().enumerate() {
            if e.kind.is_package() && !e.key.is_empty() {
                push(
                    ws,
                    ElemId {
                        src: Src::Project(f as u32),
                        elem: i as u32,
                    },
                );
            }
        }
    }
    for (f, file) in ws.lib.files.iter().enumerate() {
        for &r in &file.roots {
            if file.elements[r as usize].kind.is_package() {
                push(
                    ws,
                    ElemId {
                        src: Src::Library(f as u32),
                        elem: r,
                    },
                );
            }
        }
    }
    items
}

/// Types: every project definition, then the library's definitions and
/// (for `:>`) features.
fn type_ids(ws: &Workspace, features: bool) -> Vec<ElemId> {
    let mut ids = Vec::new();
    let wanted = |kind: &ElemKind| {
        !kind.is_package()
            && (kind.is_definition() || (features && matches!(kind, ElemKind::Usage(_))))
    };
    for (f, file) in ws.files.iter().enumerate() {
        for (i, e) in file.elements.iter().enumerate() {
            if wanted(&e.kind) && !e.key.is_empty() && !e.is_redefinition() {
                ids.push(ElemId {
                    src: Src::Project(f as u32),
                    elem: i as u32,
                });
            }
        }
    }
    for (f, file) in ws.lib.files.iter().enumerate() {
        for (i, e) in file.elements.iter().enumerate() {
            // Library features only at package level (`ISQ::mass`), not
            // the members of every library definition.
            let top_feature = e
                .parent
                .is_some_and(|p| file.elements[p as usize].kind.is_package());
            if wanted(&e.kind)
                && !e.key.is_empty()
                && !e.is_redefinition()
                && (e.kind.is_definition() || top_feature)
            {
                ids.push(ElemId {
                    src: Src::Library(f as u32),
                    elem: i as u32,
                });
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    // --- per-file (language server) ---

    #[test]
    fn completions_include_file_defs() {
        let model = parse_file("test.sysml", "part def Vehicle;\npart def Engine;\n");
        let items = completions(&model, &[], &CompletionFilter::All);
        assert!(labels(&items).contains(&"Vehicle"));
        assert!(labels(&items).contains(&"Engine"));
    }

    #[test]
    fn detect_contexts() {
        assert_eq!(
            detect_context("part engine : ", 14),
            CompletionFilter::TypePosition
        );
        assert_eq!(
            detect_context("part def Sub :> ", 16),
            CompletionFilter::Specialization
        );
        assert_eq!(
            detect_context("vehicle.", 8),
            CompletionFilter::MemberAccess("vehicle".into())
        );
        assert_eq!(
            detect_context("attribute m :> ISQ::", 20),
            CompletionFilter::QualifiedAccess("ISQ".into())
        );
        assert_eq!(detect_context("part def ", 9), CompletionFilter::All);
    }

    // --- project-wide ---

    fn complete_at(files: &[(&str, &str)], marker_file: usize) -> Completions {
        let text = files[marker_file].1;
        let offset = text.find('|').unwrap();
        let cleaned: Vec<(String, String)> = files
            .iter()
            .map(|(p, t)| (p.to_string(), t.replacen('|', "", 1)))
            .collect();
        let ws = Workspace::from_sources(cleaned.iter().map(|(p, t)| (p.as_str(), t.as_str())));
        complete(&ws, Src::Project(marker_file as u32), offset)
    }

    #[test]
    fn type_position_offers_project_and_library_types() {
        let c = complete_at(
            &[
                (
                    "a.sysml",
                    "package A { private import B::*; part def D { part battery : | } }",
                ),
                ("b.sysml", "package B { part def Battery; }"),
            ],
            0,
        );
        let l = labels(&c.items);
        assert!(l.contains(&"Battery"), "{l:?}");
        assert!(!l.contains(&"A"), "packages are not types: {l:?}");
        if !crate::stdlib::stdlib_files().is_empty() {
            assert!(l.contains(&"Real"), "{l:?}");
        }
    }

    #[test]
    fn prefix_filters_and_sets_the_range() {
        let text = "part def Battery; part def Drone { part b : Bat| }";
        let c = complete_at(&[("a.sysml", text)], 0);
        assert_eq!(c.from, text.find("Bat|").unwrap());
        assert_eq!(c.to, c.from + 3);
        assert!(labels(&c.items).contains(&"Battery"));
        assert!(!labels(&c.items).contains(&"Drone"));
    }

    #[test]
    fn member_access_lists_owned_and_inherited_members() {
        let c = complete_at(
            &[(
                "a.sysml",
                "part def C { attribute mass; }\npart def B :> C { port p; }\npart d { part b : B; attribute x = b.| ; }",
            )],
            0,
        );
        let l = labels(&c.items);
        assert!(l.contains(&"p") && l.contains(&"mass"), "{l:?}");
    }

    #[test]
    fn import_offers_packages_and_wildcards() {
        let c = complete_at(
            &[
                ("a.sysml", "package A { private import | }"),
                ("b.sysml", "package B { part def X; }"),
            ],
            0,
        );
        let l = labels(&c.items);
        assert!(l.contains(&"B") && l.contains(&"B::*"), "{l:?}");
        let c = complete_at(
            &[
                ("a.sysml", "package A { private import B::| }"),
                ("b.sysml", "package B { part def X; }"),
            ],
            0,
        );
        let l = labels(&c.items);
        assert!(l.contains(&"*") && l.contains(&"X"), "{l:?}");
    }

    #[test]
    fn statement_start_offers_keywords_and_snippets() {
        let c = complete_at(&[("a.sysml", "package A {\n    |\n}")], 0);
        let snippet = c.items.iter().find(|i| i.label == "part def").unwrap();
        assert!(snippet.is_snippet);
        assert_eq!(
            snippet.insert_text.as_deref(),
            Some("part def ${1:Name} {\n\t$0\n}")
        );
        assert!(c.items.iter().any(|i| i.kind == CompletionKind::Keyword));
        let c = complete_at(&[("a.sysml", "package A {\n    part |\n}")], 0);
        assert_eq!(labels(&c.items), ["def"]);
    }

    #[test]
    fn library_qualified_access() {
        if crate::stdlib::stdlib_files().is_empty() {
            return;
        }
        let c = complete_at(
            &[("a.sysml", "part def P { attribute m :> ISQ::ma| ; }")],
            0,
        );
        assert!(labels(&c.items).contains(&"mass"), "{:?}", labels(&c.items));
    }

    #[test]
    fn nothing_inside_comments() {
        let c = complete_at(&[("a.sysml", "part def P; /* P| */")], 0);
        assert!(c.items.is_empty());
    }
}
