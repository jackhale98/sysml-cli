//! The project index behind the editor features: every element with the
//! exact position of its name, its owner, its types and its imports, plus
//! SysML name resolution over it (owned, inherited and imported members,
//! outward through the enclosing namespaces, then the root namespace shared
//! by all files and the embedded standard library).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use super::lex::{self, Lexed, TokKind};
use crate::model::{simple_name, DefKind, Model};

/// What an element is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElemKind {
    Def(DefKind),
    /// A usage (or a KerML feature/classifier), with its keyword: `part`,
    /// `attribute`, `datatype`, ...
    Usage(String),
    /// A literal of an enumeration (`enum high;`).
    EnumLiteral,
}

/// KerML declarations the model records as usages but which declare types.
const KERML_TYPES: &[&str] = &[
    "assoc",
    "behavior",
    "class",
    "classifier",
    "datatype",
    "function",
    "interaction",
    "metaclass",
    "predicate",
    "struct",
    "type",
];

impl ElemKind {
    /// The declaration keyword(s): `part def`, `attribute`, `enum`.
    pub fn label(&self) -> String {
        match self {
            Self::Def(kind) => kind.label().to_owned(),
            Self::Usage(kind) => kind.clone(),
            Self::EnumLiteral => "enum".to_owned(),
        }
    }

    /// True for anything that declares a type (a definition, a package, a
    /// KerML classifier).
    pub fn is_definition(&self) -> bool {
        match self {
            Self::Def(_) => true,
            Self::Usage(kind) => KERML_TYPES.contains(&kind.as_str()),
            Self::EnumLiteral => false,
        }
    }

    pub fn is_package(&self) -> bool {
        matches!(self, Self::Def(DefKind::Package | DefKind::Namespace))
    }
}

/// One named element of a file.
#[derive(Debug, Clone)]
pub struct Element {
    /// The name as written (possibly quoted).
    pub name: String,
    /// The name without quotes, used for lookup.
    pub key: String,
    /// The short name (`<'SYS-001'>`), without quotes.
    pub short_name: Option<String>,
    pub kind: ElemKind,
    /// Byte span of the whole element.
    pub start: usize,
    pub end: usize,
    /// Byte span of the declared name.
    pub name_span: Option<(usize, usize)>,
    /// Where the declaration header ends: the body's `{`, or the end.
    pub header_end: usize,
    /// Byte span of the body `{ ... }`.
    pub body: Option<(usize, usize)>,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    /// Types and supertypes as written (`Battery`, `ISQ::mass`).
    pub types: Vec<String>,
    /// The redefined feature of a `:>> name` usage.
    pub redefines: Option<String>,
    pub doc: Option<String>,
}

impl Element {
    /// True when this element only redefines an inherited feature of the
    /// same name (`attribute :>> mass = 2 [kg];`).
    pub fn is_redefinition(&self) -> bool {
        self.redefines
            .as_deref()
            .is_some_and(|r| lex::unquote(simple_name(r)) == self.key)
    }

    pub fn contains(&self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }
}

/// An import statement.
#[derive(Debug, Clone)]
pub struct Import {
    /// The imported path without the `::*` / `::**` suffix.
    pub path: String,
    pub wildcard: bool,
    pub start: usize,
    pub end: usize,
    /// The namespace the import belongs to (`None`: the file root).
    pub owner: Option<u32>,
}

/// One file: its text, tokens, model and elements.
#[derive(Debug, Clone)]
pub struct FileIndex {
    pub path: String,
    pub text: String,
    pub lexed: Lexed,
    /// The parsed model (empty for standard library files).
    pub model: Model,
    pub elements: Vec<Element>,
    pub roots: Vec<u32>,
    pub imports: Vec<Import>,
    /// Name-token start -> the element it declares.
    decl_at: HashMap<usize, u32>,
}

impl FileIndex {
    /// Parse and index one project file.
    pub fn new(path: &str, text: &str) -> Self {
        let model = crate::parser::parse_file(path, text);
        let mut index = Self::build(path, text, &model, false);
        index.model = model;
        index
    }

    fn build(path: &str, text: &str, model: &Model, library: bool) -> Self {
        let lexed = lex::lex(text);
        let mut raw: Vec<Element> = Vec::new();
        for d in &model.definitions {
            raw.push(Element {
                name: d.name.clone(),
                key: lex::unquote(&d.name),
                short_name: d.short_name.as_deref().map(lex::unquote),
                kind: ElemKind::Def(d.kind),
                start: d.span.start_byte,
                end: d.span.end_byte,
                name_span: None,
                header_end: d.span.end_byte,
                body: None,
                parent: None,
                children: Vec::new(),
                types: d.super_type.iter().cloned().collect(),
                redefines: None,
                doc: d.doc.clone(),
            });
        }
        for u in &model.usages {
            // `@Meta { ... }` names a metadata type; it declares nothing.
            if u.kind == "metadata" {
                continue;
            }
            raw.push(Element {
                name: u.name.clone(),
                key: lex::unquote(&u.name),
                short_name: u.short_name.as_deref().map(lex::unquote),
                kind: ElemKind::Usage(u.kind.clone()),
                start: u.span.start_byte,
                end: u.span.end_byte,
                name_span: None,
                header_end: u.span.end_byte,
                body: None,
                parent: None,
                children: Vec::new(),
                types: u.type_ref.iter().cloned().collect(),
                redefines: u.redefinition.clone(),
                doc: u.doc.clone(),
            });
        }
        raw.retain(|e| e.end <= text.len() && e.start < e.end);
        synthesize(text, &lexed, &mut raw);
        raw.sort_by_key(|e| (e.start, std::cmp::Reverse(e.end)));
        raw.dedup_by_key(|e| (e.start, e.end));

        let mut elements = raw;
        let mut roots = Vec::new();
        let mut stack: Vec<u32> = Vec::new();
        for i in 0..elements.len() {
            let (start, end) = (elements[i].start, elements[i].end);
            while let Some(&top) = stack.last() {
                let t = &elements[top as usize];
                if t.start <= start && end <= t.end {
                    break;
                }
                stack.pop();
            }
            match stack.last() {
                Some(&p) => {
                    elements[i].parent = Some(p);
                    elements[p as usize].children.push(i as u32);
                }
                None => roots.push(i as u32),
            }
            stack.push(i as u32);
        }

        for e in &mut elements {
            scan_header(text, &lexed, e);
            if library && e.doc.is_none() {
                e.doc = preceding_comment(text, &lexed, e.start);
            }
        }

        // Enumeration literals are not in the model as usages.
        for d in &model.definitions {
            if d.enum_members.is_empty() {
                continue;
            }
            let Some(owner) = elements
                .iter()
                .position(|e| e.start == d.span.start_byte && e.kind == ElemKind::Def(d.kind))
            else {
                continue;
            };
            let body_start = elements[owner].header_end;
            let end = elements[owner].end;
            for member in &d.enum_members {
                let key = lex::unquote(&member.name);
                let first = lexed.token_after(body_start);
                let tok = lexed.tokens[first..]
                    .iter()
                    .take_while(|t| t.start < end)
                    .find(|t| t.is_name() && lex::unquote(t.text(text)) == key);
                let Some(tok) = tok else { continue };
                let child = elements.len() as u32;
                elements.push(Element {
                    name: member.name.clone(),
                    key,
                    short_name: None,
                    kind: ElemKind::EnumLiteral,
                    start: tok.start,
                    end: tok.end,
                    name_span: Some((tok.start, tok.end)),
                    header_end: tok.end,
                    body: None,
                    parent: Some(owner as u32),
                    children: Vec::new(),
                    types: Vec::new(),
                    redefines: None,
                    doc: member.doc.clone(),
                });
                elements[owner].children.push(child);
            }
        }

        let mut imports = Vec::new();
        for imp in &model.imports {
            let owner = innermost(&elements, imp.span.start_byte);
            imports.push(Import {
                path: imp.path.clone(),
                wildcard: imp.is_wildcard || imp.is_recursive,
                start: imp.span.start_byte,
                end: imp.span.end_byte,
                owner,
            });
        }

        let decl_at = elements
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.name_span.map(|(s, _)| (s, i as u32)))
            .collect();

        Self {
            path: path.to_owned(),
            text: text.to_owned(),
            lexed,
            model: Model::new(path.to_owned()),
            elements,
            roots,
            imports,
            decl_at,
        }
    }

    /// The element whose name is declared by the token starting at `start`.
    pub fn declared_at(&self, start: usize) -> Option<u32> {
        self.decl_at.get(&start).copied()
    }

    /// The innermost element containing `offset`.
    pub fn innermost(&self, offset: usize) -> Option<u32> {
        innermost(&self.elements, offset)
    }

    /// The token under the cursor: the one containing `offset`, else a
    /// name ending exactly at `offset` (cursor just after a word).
    pub fn token_at(&self, offset: usize) -> Option<usize> {
        let tokens = &self.lexed.tokens;
        let i = self.lexed.token_after(offset);
        if let Some(t) = tokens.get(i) {
            if t.start <= offset && t.is_name() {
                return Some(i);
            }
        }
        if i > 0 && tokens[i - 1].end == offset && tokens[i - 1].is_name() {
            return Some(i - 1);
        }
        tokens.get(i).filter(|t| t.start <= offset).map(|_| i)
    }

    /// The import statement containing `offset`.
    pub fn import_at(&self, offset: usize) -> Option<&Import> {
        self.imports
            .iter()
            .find(|i| i.start <= offset && offset < i.end)
    }
}

fn innermost(elements: &[Element], offset: usize) -> Option<u32> {
    elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.contains(offset))
        .min_by_key(|(_, e)| e.end - e.start)
        .map(|(i, _)| i as u32)
}

/// Keywords that declare a feature (`perform action x`, `exhibit state m`,
/// KerML `abstract function sum`) — used to find declarations the model
/// does not record.
const DECLARING: &[&str] = &[
    "action",
    "attribute",
    "behavior",
    "calc",
    "class",
    "constraint",
    "datatype",
    "feature",
    "function",
    "interface",
    "item",
    "occurrence",
    "part",
    "port",
    "predicate",
    "requirement",
    "state",
    "struct",
];

/// Add elements for `<keyword> <name>` declarations the model missed.
fn synthesize(text: &str, lexed: &Lexed, raw: &mut Vec<Element>) {
    let tokens = &lexed.tokens;
    let mut starts: Vec<usize> = raw.iter().map(|e| e.start).collect();
    starts.sort_unstable();
    let mut stmt_start = 0;
    for (i, t) in tokens.iter().enumerate() {
        let s = t.text(text);
        if t.kind == TokKind::Punct && matches!(s, ";" | "{" | "}") {
            stmt_start = t.end;
            continue;
        }
        if t.kind != TokKind::Keyword || !DECLARING.contains(&s) {
            continue;
        }
        let Some(name) = tokens.get(i + 1).filter(|n| n.is_name()) else {
            continue;
        };
        // Recorded by the model: an element starts in this statement.
        let first = starts.partition_point(|&st| st < stmt_start);
        if starts.get(first).is_some_and(|&st| st < name.end) {
            continue;
        }
        // The extent: to the `;`, or over the `{ ... }` body.
        let mut end = None;
        let mut depth = 0i32;
        for u in &tokens[i + 2..] {
            match u.text(text) {
                "(" | "[" if u.kind == TokKind::Punct => depth += 1,
                ")" | "]" if u.kind == TokKind::Punct => depth -= 1,
                "{" if u.kind == TokKind::Punct => depth += 1,
                "}" if u.kind == TokKind::Punct => {
                    if depth <= 0 {
                        // The enclosing body ends: an unterminated statement.
                        end = Some(u.start);
                        break;
                    }
                    depth -= 1;
                    if depth == 0 {
                        end = Some(u.end);
                        break;
                    }
                }
                ";" if u.kind == TokKind::Punct && depth == 0 => {
                    end = Some(u.end);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else { continue };
        let name_text = name.text(text);
        raw.push(Element {
            name: name_text.to_owned(),
            key: lex::unquote(name_text),
            short_name: None,
            kind: ElemKind::Usage(s.to_owned()),
            start: t.start,
            end,
            name_span: None,
            header_end: end,
            body: None,
            parent: None,
            children: Vec::new(),
            types: Vec::new(),
            redefines: None,
            doc: None,
        });
        let at = starts.partition_point(|&st| st < t.start);
        starts.insert(at, t.start);
    }
}

/// Find the element's name token, body and lexical types.
fn scan_header(text: &str, lexed: &Lexed, e: &mut Element) {
    let tokens = &lexed.tokens;
    let first = lexed.token_after(e.start);
    let mut depth = 0i32;
    let mut i = first;
    let mut header_tokens = Vec::new();
    while let Some(t) = tokens.get(i) {
        if t.start >= e.end {
            break;
        }
        if t.kind == TokKind::Punct {
            match t.text(text) {
                "(" | "[" => depth += 1,
                ")" | "]" => depth -= 1,
                "{" if depth <= 0 => {
                    e.header_end = t.start;
                    e.body = Some((t.start, e.end));
                    break;
                }
                ";" if depth <= 0 => break,
                _ => {}
            }
        }
        header_tokens.push(i);
        i += 1;
    }

    // The name: the first name token equal to the key, outside `<...>`.
    let mut k = 0;
    let mut in_expr = false;
    while k < header_tokens.len() {
        let t = tokens[header_tokens[k]];
        let s = t.text(text);
        if t.is_punct(text, "<") && !in_expr {
            // Skip a short name `<'X-1'>`.
            if let Some(close) = header_tokens[k..]
                .iter()
                .position(|&j| tokens[j].is_punct(text, ">"))
            {
                k += close + 1;
                continue;
            }
        }
        if matches!(s, "=" | ":=") || t.is_keyword(text, "default") {
            in_expr = true;
        }
        if !in_expr && t.is_name() && !e.key.is_empty() && lex::unquote(s) == e.key {
            e.name_span = Some((t.start, t.end));
            break;
        }
        k += 1;
    }

    // Types written after `:`, `:>`, `specializes`, `subsets`, ...
    let mut lexical = Vec::new();
    let mut k = 0;
    let mut expect = false;
    while k < header_tokens.len() {
        let t = tokens[header_tokens[k]];
        let s = t.text(text);
        if matches!(s, "=" | ":=") || t.is_keyword(text, "default") {
            break;
        }
        if matches!(s, ":" | ":>" | "::>")
            || t.is_keyword(text, "specializes")
            || t.is_keyword(text, "subsets")
            || (t.is_keyword(text, "by")
                && k > 0
                && tokens[header_tokens[k - 1]].is_keyword(text, "defined"))
        {
            expect = true;
            k += 1;
            continue;
        }
        if expect && (s == "~" || s == ",") {
            k += 1;
            continue;
        }
        if expect && t.is_name() {
            let mut path = s.to_owned();
            while k + 2 < header_tokens.len() {
                let sep = tokens[header_tokens[k + 1]];
                let next = tokens[header_tokens[k + 2]];
                if (sep.is_punct(text, "::") || sep.is_punct(text, ".")) && next.is_name() {
                    path.push_str(sep.text(text));
                    path.push_str(next.text(text));
                    k += 2;
                } else {
                    break;
                }
            }
            lexical.push(path);
            // `:> A, B` continues with a comma; anything else ends the list.
            let more = header_tokens
                .get(k + 1)
                .is_some_and(|&j| tokens[j].is_punct(text, ","));
            expect = more;
            k += 1;
            continue;
        }
        expect = false;
        k += 1;
    }
    for t in lexical {
        if !e.types.contains(&t) {
            e.types.push(t);
        }
    }
}

/// The block comment directly before `offset` (only whitespace between),
/// as documentation text.
fn preceding_comment(text: &str, lexed: &Lexed, offset: usize) -> Option<String> {
    let i = lexed.comments.partition_point(|c| c.end <= offset);
    let c = lexed.comments.get(i.checked_sub(1)?)?;
    if !c.block || !text[c.end..offset].trim().is_empty() {
        return None;
    }
    let doc = comment_text(&text[c.start..c.end]);
    (!doc.is_empty()).then_some(doc)
}

/// The text of a `/* ... */` comment without its markers.
pub fn comment_text(comment: &str) -> String {
    let inner = comment
        .trim()
        .trim_start_matches("/*")
        .trim_end_matches("*/");
    inner
        .lines()
        .map(|l| {
            let l = l.trim();
            l.strip_prefix('*').map(str::trim_start).unwrap_or(l)
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// Where an element lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Src {
    /// A project file, by its index in the workspace.
    Project(u32),
    /// A standard library file, by its index in [`library`].
    Library(u32),
}

/// An element of the workspace or the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ElemId {
    pub src: Src,
    pub elem: u32,
}

/// The embedded OMG standard library, indexed once.
pub struct Library {
    pub files: Vec<FileIndex>,
    roots: HashMap<String, Vec<ElemId>>,
    by_name: HashMap<String, Vec<ElemId>>,
}

/// The indexed standard library (built on first use).
pub fn library() -> &'static Library {
    static LIBRARY: OnceLock<Library> = OnceLock::new();
    LIBRARY.get_or_init(|| {
        let models = crate::stdlib::parse_stdlib();
        let files: Vec<FileIndex> = crate::stdlib::stdlib_files()
            .iter()
            .zip(models)
            .map(|((path, text), model)| FileIndex::build(path, text, model, true))
            .collect();
        let (roots, by_name) = name_tables(files.iter(), Src::Library);
        Library {
            files,
            roots,
            by_name,
        }
    })
}

type NameTable = HashMap<String, Vec<ElemId>>;

fn name_tables<'a>(
    files: impl Iterator<Item = &'a FileIndex>,
    src: fn(u32) -> Src,
) -> (NameTable, NameTable) {
    let mut roots: NameTable = HashMap::new();
    let mut by_name: NameTable = HashMap::new();
    for (f, file) in files.enumerate() {
        let src = src(f as u32);
        for &r in &file.roots {
            let e = &file.elements[r as usize];
            if !e.key.is_empty() {
                roots
                    .entry(e.key.clone())
                    .or_default()
                    .push(ElemId { src, elem: r });
            }
        }
        for (i, e) in file.elements.iter().enumerate() {
            if e.key.is_empty() || e.is_redefinition() {
                continue;
            }
            by_name.entry(e.key.clone()).or_default().push(ElemId {
                src,
                elem: i as u32,
            });
        }
    }
    (roots, by_name)
}

/// A project (its files) plus the standard library, with name resolution.
pub struct Workspace {
    pub files: Vec<Arc<FileIndex>>,
    pub lib: &'static Library,
    roots: NameTable,
    by_name: NameTable,
    types: RefCell<HashMap<ElemId, Vec<ElemId>>>,
}

/// A name path around the cursor: `a.b.c` or `P::Q::R`.
#[derive(Debug, Clone)]
pub struct NamePath {
    /// Token indices of the segments.
    pub segs: Vec<usize>,
    /// Which segment holds the cursor.
    pub cursor: usize,
}

impl Workspace {
    pub fn new(files: Vec<Arc<FileIndex>>) -> Self {
        let (roots, mut by_name) = name_tables(files.iter().map(|f| f.as_ref()), Src::Project);
        // Prefer definitions when a simple name is ambiguous.
        for ids in by_name.values_mut() {
            ids.sort_by_key(|id| {
                let f = &files[match id.src {
                    Src::Project(f) => f as usize,
                    Src::Library(_) => unreachable!(),
                }];
                !f.elements[id.elem as usize].kind.is_definition()
            });
        }
        Self {
            files,
            lib: library(),
            roots,
            by_name,
            types: RefCell::new(HashMap::new()),
        }
    }

    /// Index files given as `(path, text)`.
    pub fn from_sources<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Self::new(
            files
                .into_iter()
                .map(|(p, t)| Arc::new(FileIndex::new(p, t)))
                .collect(),
        )
    }

    pub fn file(&self, src: Src) -> &FileIndex {
        match src {
            Src::Project(f) => &self.files[f as usize],
            Src::Library(f) => &self.lib.files[f as usize],
        }
    }

    pub fn elem(&self, id: ElemId) -> &Element {
        &self.file(id.src).elements[id.elem as usize]
    }

    /// The project file with this path.
    pub fn find_file(&self, path: &str) -> Option<Src> {
        self.files
            .iter()
            .position(|f| f.path == path)
            .map(|i| Src::Project(i as u32))
    }

    pub fn parent(&self, id: ElemId) -> Option<ElemId> {
        self.elem(id).parent.map(|p| ElemId {
            src: id.src,
            elem: p,
        })
    }

    pub fn children(&self, id: ElemId) -> impl Iterator<Item = ElemId> + '_ {
        self.elem(id).children.iter().map(move |&c| ElemId {
            src: id.src,
            elem: c,
        })
    }

    /// `Pkg::Def::member`, quoting names that need it.
    pub fn qualified_name(&self, id: ElemId) -> String {
        let mut segs = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            let e = self.elem(c);
            if !e.key.is_empty() {
                segs.push(lex::quote_name(&e.key));
            }
            cur = self.parent(c);
        }
        segs.reverse();
        segs.join("::")
    }

    /// The namespace names at `offset` resolve in: the innermost element,
    /// or its owner when `offset` is in the element's own declaration
    /// header (`part def A :> B` resolves `B` outside `A`).
    pub fn scope_at(&self, src: Src, offset: usize) -> Option<ElemId> {
        let file = self.file(src);
        let e = file.innermost(offset)?;
        let el = &file.elements[e as usize];
        let e = if offset < el.header_end {
            el.parent?
        } else {
            e
        };
        Some(ElemId { src, elem: e })
    }

    /// Resolve a simple name from a scope: owned, inherited and imported
    /// members of each enclosing namespace, the file's root imports, then
    /// the root namespace of the project and the library.
    pub fn lookup(&self, src: Src, scope: Option<ElemId>, name: &str) -> Option<ElemId> {
        let mut visited = HashSet::new();
        let mut cur = scope;
        while let Some(ns) = cur {
            if let Some(found) = self.member_in(ns, name, &mut visited, 0) {
                return Some(found);
            }
            cur = self.parent(ns);
        }
        let file = self.file(src);
        for imp in file.imports.iter().filter(|i| i.owner.is_none()) {
            if let Some(found) = self.via_import(imp, name, &mut visited, 0) {
                return Some(found);
            }
        }
        self.root(name)
    }

    /// A top-level element of the project or the library.
    pub fn root(&self, name: &str) -> Option<ElemId> {
        self.roots
            .get(name)
            .or_else(|| self.lib.roots.get(name))
            .and_then(|ids| ids.first().copied())
    }

    /// Any element with this simple name — a last resort for models that
    /// rely on names being visible without imports. Definitions first,
    /// project before library.
    pub fn global(&self, name: &str) -> Option<ElemId> {
        self.by_name
            .get(name)
            .and_then(|ids| ids.first().copied())
            .or_else(|| {
                self.lib.by_name.get(name).and_then(|ids| {
                    ids.iter()
                        .copied()
                        .find(|&id| self.elem(id).kind.is_definition())
                        .or_else(|| ids.first().copied())
                })
            })
    }

    /// Every project element named `name` (definitions first).
    pub fn project_named(&self, name: &str) -> &[ElemId] {
        self.by_name.get(name).map_or(&[], Vec::as_slice)
    }

    /// Every library element named `name`.
    pub fn library_named(&self, name: &str) -> &[ElemId] {
        self.lib.by_name.get(name).map_or(&[], Vec::as_slice)
    }

    /// A member of namespace or type `ns`: owned, inherited or imported.
    pub fn member(&self, ns: ElemId, name: &str) -> Option<ElemId> {
        self.member_in(ns, name, &mut HashSet::new(), 0)
    }

    fn member_in(
        &self,
        ns: ElemId,
        name: &str,
        visited: &mut HashSet<ElemId>,
        depth: usize,
    ) -> Option<ElemId> {
        if depth > 16 || !visited.insert(ns) {
            return None;
        }
        for c in self.children(ns) {
            let e = self.elem(c);
            if e.key == name || e.short_name.as_deref() == Some(name) {
                return Some(c);
            }
        }
        for t in self.types_of(ns) {
            if let Some(found) = self.member_in(t, name, visited, depth + 1) {
                return Some(found);
            }
        }
        let file = self.file(ns.src);
        for imp in file.imports.iter().filter(|i| i.owner == Some(ns.elem)) {
            if let Some(found) = self.via_import(imp, name, visited, depth + 1) {
                return Some(found);
            }
        }
        None
    }

    fn via_import(
        &self,
        imp: &Import,
        name: &str,
        visited: &mut HashSet<ElemId>,
        depth: usize,
    ) -> Option<ElemId> {
        if imp.wildcard {
            let target = self.resolve_qualified(&imp.path)?;
            self.member_in(target, name, visited, depth)
        } else if lex::unquote(last_segment(&imp.path)) == name {
            self.resolve_qualified(&imp.path)
        } else {
            None
        }
    }

    /// Resolve `A::B::C` from the root namespace (falling back to any
    /// element named `A` for the first segment).
    pub fn resolve_qualified(&self, path: &str) -> Option<ElemId> {
        let segs = split_path(path);
        let first = lex::unquote(segs.first()?);
        let mut cur = self.root(&first).or_else(|| self.global(&first))?;
        for seg in &segs[1..] {
            cur = self.member(cur, &lex::unquote(seg))?;
        }
        Some(cur)
    }

    /// Resolve a path written in `src` with `scope` as the namespace.
    pub fn resolve_path(&self, src: Src, scope: Option<ElemId>, path: &str) -> Option<ElemId> {
        let segs = split_path(path);
        let first = lex::unquote(segs.first()?);
        let mut cur = self
            .lookup(src, scope, &first)
            .or_else(|| self.global(&first))?;
        for seg in &segs[1..] {
            cur = self.member(cur, &lex::unquote(seg))?;
        }
        Some(cur)
    }

    /// The resolved types and supertypes of an element (cached).
    pub fn types_of(&self, id: ElemId) -> Vec<ElemId> {
        if let Some(t) = self.types.borrow().get(&id) {
            return t.clone();
        }
        // Placeholder: breaks cycles (`part def A :> A`).
        self.types.borrow_mut().insert(id, Vec::new());
        let e = self.elem(id);
        let scope = self.parent(id);
        let resolved: Vec<ElemId> = e
            .types
            .iter()
            .filter_map(|t| self.resolve_path(id.src, scope, t))
            .filter(|&t| t != id)
            .collect();
        self.types.borrow_mut().insert(id, resolved.clone());
        resolved
    }

    /// What a declared element stands for: a `:>> x` redefinition resolves
    /// to the inherited `x` it redefines, anything else to itself.
    pub fn declared(&self, id: ElemId) -> ElemId {
        let e = self.elem(id);
        if !e.is_redefinition() {
            return id;
        }
        if let Some(owner) = self.parent(id) {
            for t in self.types_of(owner) {
                if let Some(found) = self.member(t, &e.key) {
                    if found != id {
                        return found;
                    }
                }
            }
        }
        id
    }

    /// The original feature behind chains of redefinitions.
    pub fn canonical(&self, mut id: ElemId) -> ElemId {
        for _ in 0..16 {
            let next = self.declared(id);
            if next == id {
                break;
            }
            id = next;
        }
        id
    }

    /// The `a.b.c` / `P::Q` path around name token `tok`.
    pub fn path_at(&self, src: Src, tok: usize) -> NamePath {
        let file = self.file(src);
        let tokens = &file.lexed.tokens;
        let text = &file.text;
        let is_sep = |i: usize| tokens[i].is_punct(text, "::") || tokens[i].is_punct(text, ".");
        let mut first = tok;
        while first >= 2 && is_sep(first - 1) && tokens[first - 2].is_name() {
            first -= 2;
        }
        let mut last = tok;
        while last + 2 < tokens.len() && is_sep(last + 1) && tokens[last + 2].is_name() {
            last += 2;
        }
        NamePath {
            segs: (first..=last).step_by(2).collect(),
            cursor: (tok - first) / 2,
        }
    }

    /// Resolve segment `k` of a path.
    pub fn resolve_segment(&self, src: Src, path: &NamePath, k: usize) -> Option<ElemId> {
        let file = self.file(src);
        let tok = file.lexed.tokens[path.segs[k]];
        let key = lex::unquote(tok.text(&file.text));
        if k > 0 {
            let prev = self.resolve_segment(src, path, k - 1)?;
            return self.member(prev, &key);
        }
        if file.import_at(tok.start).is_some() {
            return self.root(&key).or_else(|| self.global(&key));
        }
        if path.segs.len() == 1 {
            if let Some(e) = file.declared_at(tok.start) {
                return Some(self.declared(ElemId { src, elem: e }));
            }
        }
        let scope = self.scope_at(src, tok.start);
        self.lookup(src, scope, &key).or_else(|| self.global(&key))
    }

    /// The element the name at `offset` refers to (or declares), with the
    /// name token's byte span.
    pub fn resolve_at(&self, src: Src, offset: usize) -> Option<(ElemId, (usize, usize))> {
        let file = self.file(src);
        if file.lexed.comment_at(offset).is_some() {
            return None;
        }
        let ti = file.token_at(offset)?;
        let tok = file.lexed.tokens[ti];
        if !tok.is_name() {
            return None;
        }
        let path = self.path_at(src, ti);
        let id = self.resolve_segment(src, &path, path.cursor)?;
        Some((id, (tok.start, tok.end)))
    }

    /// Every name token in the project that refers to (or declares) the
    /// same element as `target`, after following redefinitions.
    pub fn occurrences(&self, target: ElemId) -> Vec<(Src, usize, usize)> {
        let target = self.canonical(target);
        let key = &self.elem(target).key;
        let mut out = Vec::new();
        for (f, file) in self.files.iter().enumerate() {
            let src = Src::Project(f as u32);
            for (ti, tok) in file.lexed.tokens.iter().enumerate() {
                if !tok.is_name() || lex::unquote(tok.text(&file.text)) != *key {
                    continue;
                }
                let path = self.path_at(src, ti);
                if let Some(id) = self.resolve_segment(src, &path, path.cursor) {
                    if self.canonical(id) == target {
                        out.push((src, tok.start, tok.end));
                    }
                }
            }
        }
        out
    }

    /// Members of a namespace or type: owned first, then inherited, then
    /// imported; one per name.
    pub fn members_of(&self, ns: ElemId) -> Vec<ElemId> {
        let mut out = Vec::new();
        let mut names = HashSet::new();
        self.collect_members(ns, &mut out, &mut names, &mut HashSet::new(), 0);
        out
    }

    fn collect_members(
        &self,
        ns: ElemId,
        out: &mut Vec<ElemId>,
        names: &mut HashSet<String>,
        visited: &mut HashSet<ElemId>,
        depth: usize,
    ) {
        if depth > 8 || !visited.insert(ns) {
            return;
        }
        for c in self.children(ns) {
            let e = self.elem(c);
            if !e.key.is_empty() && names.insert(e.key.clone()) {
                out.push(c);
            }
        }
        for t in self.types_of(ns) {
            self.collect_members(t, out, names, visited, depth + 1);
        }
        let file = self.file(ns.src);
        let imports: Vec<&Import> = file
            .imports
            .iter()
            .filter(|i| i.owner == Some(ns.elem))
            .collect();
        for imp in imports {
            self.collect_import(imp, out, names, visited, depth + 1);
        }
    }

    fn collect_import(
        &self,
        imp: &Import,
        out: &mut Vec<ElemId>,
        names: &mut HashSet<String>,
        visited: &mut HashSet<ElemId>,
        depth: usize,
    ) {
        let Some(target) = self.resolve_qualified(&imp.path) else {
            return;
        };
        if imp.wildcard {
            self.collect_members(target, out, names, visited, depth);
        } else if names.insert(self.elem(target).key.clone()) {
            out.push(target);
        }
    }

    /// Every name visible at a scope, innermost first.
    pub fn visible(&self, src: Src, scope: Option<ElemId>) -> Vec<ElemId> {
        let mut out = Vec::new();
        let mut names = HashSet::new();
        let mut visited = HashSet::new();
        let mut cur = scope;
        while let Some(ns) = cur {
            self.collect_members(ns, &mut out, &mut names, &mut visited, 0);
            cur = self.parent(ns);
        }
        let file = self.file(src);
        for imp in file.imports.iter().filter(|i| i.owner.is_none()) {
            self.collect_import(imp, &mut out, &mut names, &mut visited, 0);
        }
        for f in 0..self.files.len() {
            let src = Src::Project(f as u32);
            for &r in &self.file(src).roots {
                let id = ElemId { src, elem: r };
                if names.insert(self.elem(id).key.clone()) {
                    out.push(id);
                }
            }
        }
        out
    }

    /// 1-based line of an element's name (or start).
    pub fn line_of(&self, id: ElemId) -> usize {
        let file = self.file(id.src);
        let e = self.elem(id);
        let at = e.name_span.map_or(e.start, |(s, _)| s);
        file.text[..at].matches('\n').count() + 1
    }

    /// The declaration header as written, on one line: `part def Battery
    /// :> Component`.
    pub fn header(&self, id: ElemId) -> String {
        let file = self.file(id.src);
        let e = self.elem(id);
        if e.kind == ElemKind::EnumLiteral {
            return format!("enum {}", e.name);
        }
        let mut out = String::new();
        let first = file.lexed.token_after(e.start);
        let tokens: Vec<&str> = file.lexed.tokens[first..]
            .iter()
            .take_while(|t| t.start < e.header_end)
            .map(|t| t.text(&file.text))
            .take_while(|&s| s != ";")
            .collect();
        let mut short_name = false;
        for (k, &s) in tokens.iter().enumerate() {
            // `<'SYS-1'>` is written without inner spaces.
            if s == "<" && tokens.get(k + 2) == Some(&">") {
                short_name = true;
            }
            let glue = matches!(s, "::" | "." | "," | "[" | "]" | ")" | "(")
                || (s == ">" && short_name)
                || out.ends_with("::")
                || out.ends_with('.')
                || out.ends_with('~')
                || out.ends_with('[')
                || out.ends_with('(')
                || (out.ends_with('<') && short_name);
            if s == ">" {
                short_name = false;
            }
            if !out.is_empty() && !glue {
                out.push(' ');
            }
            out.push_str(s);
            if out.len() > 240 {
                out.push_str(" …");
                break;
            }
        }
        out
    }
}

/// Split `A::B::'C D'` into segments (quoted names may contain `::`).
pub fn split_path(path: &str) -> Vec<&str> {
    let mut segs = Vec::new();
    let mut start = 0;
    let mut in_quote = false;
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_quote => i += 1,
            b'\'' => in_quote = !in_quote,
            b':' if !in_quote && bytes.get(i + 1) == Some(&b':') => {
                segs.push(path[start..i].trim());
                i += 1;
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    segs.push(path[start.min(path.len())..].trim());
    segs.retain(|s| !s.is_empty());
    segs
}

fn last_segment(path: &str) -> &str {
    split_path(path).last().copied().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(files: &[(&str, &str)]) -> Workspace {
        Workspace::from_sources(files.iter().copied())
    }

    fn at(text: &str, needle: &str) -> usize {
        text.find(needle).unwrap()
    }

    #[test]
    fn elements_have_name_spans_and_owners() {
        let text = "package P {\n    part def 'Battery pack' :> Base { attribute m : Real; }\n    part def Base;\n    requirement <'R-1'> r : Base [1] { }\n}\n";
        let ws = ws(&[("a.sysml", text)]);
        let f = &ws.files[0];
        let r = f.elements.iter().position(|e| e.key == "r").unwrap();
        let r = ElemId {
            src: Src::Project(0),
            elem: r as u32,
        };
        assert_eq!(ws.header(r), "requirement <'R-1'> r : Base[1]");
        let bp = f.elements.iter().find(|e| e.key == "Battery pack").unwrap();
        let (s, e) = bp.name_span.unwrap();
        assert_eq!(&text[s..e], "'Battery pack'");
        assert_eq!(bp.types, ["Base"]);
        let m = f.elements.iter().find(|e| e.key == "m").unwrap();
        assert_eq!(f.elements[m.parent.unwrap() as usize].key, "Battery pack");
    }

    #[test]
    fn resolution_follows_imports_and_inheritance() {
        let a = "package A {\n    private import B::*;\n    part def Drone { part battery : Battery; }\n    part d : Drone { attribute x = battery.cap; }\n}\n";
        let b = "package B {\n    part def Battery { attribute cap; }\n}\n";
        let ws = ws(&[("a.sysml", a), ("b.sysml", b)]);
        let src = Src::Project(0);
        let (id, _) = ws.resolve_at(src, at(a, "Battery;")).unwrap();
        assert_eq!(ws.qualified_name(id), "B::Battery");
        assert_eq!(ws.files[1].path, "b.sysml");
        let (cap, _) = ws.resolve_at(src, at(a, "cap;")).unwrap();
        assert_eq!(ws.qualified_name(cap), "B::Battery::cap");
        let (battery, _) = ws.resolve_at(src, at(a, "battery.cap")).unwrap();
        assert_eq!(ws.qualified_name(battery), "A::Drone::battery");
    }

    #[test]
    fn redefinitions_resolve_to_the_redefined_feature() {
        let a = "part def C { attribute mass; }\npart def D :> C { attribute :>> mass = 2; }\npart x : D { attribute y = mass; }\n";
        let ws = ws(&[("a.sysml", a)]);
        let src = Src::Project(0);
        let (redef, _) = ws.resolve_at(src, at(a, "mass = 2")).unwrap();
        assert_eq!(ws.qualified_name(redef), "C::mass");
        let occ = ws.occurrences(redef);
        assert_eq!(occ.len(), 3, "{occ:?}");
    }

    #[test]
    fn library_names_resolve() {
        if crate::stdlib::stdlib_files().is_empty() {
            return;
        }
        let a = "package A { private import ISQ::*; attribute m :> mass; attribute r : ScalarValues::Real; }";
        let ws = ws(&[("a.sysml", a)]);
        let (mass, _) = ws.resolve_at(Src::Project(0), at(a, "mass;")).unwrap();
        assert!(matches!(mass.src, Src::Library(_)));
        assert_eq!(ws.elem(mass).key, "mass");
        let (real, _) = ws.resolve_at(Src::Project(0), at(a, "Real")).unwrap();
        assert_eq!(ws.qualified_name(real), "ScalarValues::Real");
    }

    #[test]
    fn declarations_missing_from_the_model_are_indexed() {
        let a = "part def D { perform action fly : Fly; exhibit state modes; }\naction def Fly;\npart d : D { attribute x = fly; }";
        let ws = ws(&[("a.sysml", a)]);
        let f = &ws.files[0];
        let fly = f.elements.iter().find(|e| e.key == "fly").unwrap();
        assert_eq!(fly.kind, ElemKind::Usage("action".into()));
        assert_eq!(fly.types, ["Fly"]);
        assert_eq!(f.elements[fly.parent.unwrap() as usize].key, "D");
        assert!(f.elements.iter().any(|e| e.key == "modes"));
        let (id, _) = ws
            .resolve_at(Src::Project(0), a.rfind("fly").unwrap())
            .unwrap();
        assert_eq!(ws.qualified_name(id), "D::fly");
    }

    #[test]
    fn names_in_comments_do_not_resolve() {
        let a = "part def A; // A\npart x : A;";
        let ws = ws(&[("a.sysml", a)]);
        assert!(ws
            .resolve_at(Src::Project(0), a.find("// A").unwrap() + 3)
            .is_none());
        let def = ws.resolve_at(Src::Project(0), 9).unwrap().0;
        assert_eq!(ws.occurrences(def).len(), 2);
    }

    #[test]
    fn paths_split_on_double_colons_outside_quotes() {
        assert_eq!(split_path("A::'B::C'::D"), ["A", "'B::C'", "D"]);
        assert_eq!(split_path("A"), ["A"]);
    }
}
