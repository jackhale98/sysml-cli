//! The semantic model API: building a resolved model from source files plus
//! the standard library, and the queries consumers (checks, diagrams,
//! editor services, export) use.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::graph::{
    Element, ElementRef, FileId, Flags, Graph, RefId, Reference, Role, Span, Target,
};
use crate::id::{self, ElementId, OwnershipTree, Spec};
use crate::lower;
use crate::metaclass::Metaclass as M;
use crate::metaclass::Metaclass;
use crate::name::{self, OwningNamespace};
use crate::resolve::{
    Access, Caches, Entry, Frozen, RefState, Resolution, Resolver, Slot, State, TableKey,
};

/// A source file given to the model builder.
#[derive(Clone, Copy, Debug)]
pub struct SourceFile<'a> {
    /// A stable, project-relative path (also used to tell KerML `.kerml`
    /// from SysML files).
    pub path: &'a str,
    pub text: &'a str,
}

/// The kind of a semantic diagnostic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiagnosticKind {
    /// A qualified name or feature chain segment that does not resolve.
    Unresolved,
}

/// A problem found while resolving the model.
#[derive(Clone, PartialEq, Debug)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    /// Where the unresolved name is written.
    pub span: Span,
    /// The name as written (`A::b`), or the chain segment that failed.
    pub name: String,
    /// The reference that failed.
    pub reference: RefId,
    pub message: String,
}

/// A membership of a namespace, as returned by member queries.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Member {
    pub membership: ElementRef,
    pub element: ElementRef,
    pub visibility: crate::graph::Visibility,
    pub origin: crate::resolve::Origin,
}

impl From<&Entry> for Member {
    fn from(e: &Entry) -> Self {
        Member {
            membership: e.membership,
            element: e.element,
            visibility: e.visibility,
            origin: e.origin,
        }
    }
}

/// Build timings, for performance reporting.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuildStats {
    pub parse_ms: f64,
    pub lower_ms: f64,
    pub resolve_ms: f64,
    pub elements: usize,
    pub references: usize,
}

/// The standard library (or another set of library files), lowered and
/// fully resolved once, shared by every model built on it.
#[derive(Debug)]
pub struct Library {
    pub(crate) frozen: Frozen,
    /// Specification family of each library file (for element ids).
    specs: Vec<Spec>,
    unresolved: Vec<Diagnostic>,
    stats: BuildStats,
}

/// Elapsed milliseconds from a start point (0 when no clock is available,
/// e.g. on `wasm32-unknown-unknown`).
pub(crate) struct Clock(#[allow(dead_code)] Option<std::time::Instant>);

impl Clock {
    pub fn start() -> Clock {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Clock(Some(std::time::Instant::now()))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Clock(None)
        }
    }
    pub fn ms(&self) -> f64 {
        match self.0 {
            Some(t) => t.elapsed().as_secs_f64() * 1000.0,
            None => 0.0,
        }
    }
}

impl Library {
    /// The embedded OMG standard library (built on first use, then cached).
    pub fn standard() -> Arc<Library> {
        static LIB: OnceLock<Arc<Library>> = OnceLock::new();
        LIB.get_or_init(|| {
            let files: Vec<SourceFile> = sysml_syntax::stdlib_files()
                .iter()
                .map(|(path, text)| SourceFile { path, text })
                .collect();
            Arc::new(Library::build(&files))
        })
        .clone()
    }

    /// Lower and resolve library files.
    pub fn build(files: &[SourceFile]) -> Library {
        let mut graph = Graph::default();
        let (parse_ms, lower_ms) = lower_files(&mut graph, files);
        let mut state = State::default();
        let mut cache = Caches::default();
        let clock = Clock::start();
        {
            let mut r = Resolver {
                lib: None,
                g: Access::Write(&mut graph),
                st: Access::Write(&mut state),
                cache: &mut cache,
                cuts: 0,
            };
            finish(&mut r);
            freeze_derived(&mut r);
        }
        let resolve_ms = clock.ms();
        let unresolved = unresolved_of(&graph, &state);
        let specs = files
            .iter()
            .map(|f| {
                if f.path.starts_with("Kernel Libraries") || f.path.contains("Kernel Libraries/") {
                    Spec::KerML
                } else {
                    Spec::SysML
                }
            })
            .collect();
        let stats = BuildStats {
            parse_ms,
            lower_ms,
            resolve_ms,
            elements: graph.len(),
            references: graph.refs.len(),
        };
        Library {
            frozen: Frozen {
                graph,
                state,
                tables: cache.tables,
                all_supertypes: cache.all_supertypes,
                lib_cache: cache.lib_cache,
            },
            specs,
            unresolved,
            stats,
        }
    }

    /// An empty library (for testing name resolution without the standard library).
    pub fn empty() -> Arc<Library> {
        Arc::new(Library::build(&[]))
    }

    pub fn stats(&self) -> BuildStats {
        self.stats
    }

    /// References in the library text that did not resolve.
    pub fn unresolved(&self) -> &[Diagnostic] {
        &self.unresolved
    }

    pub fn len(&self) -> usize {
        self.frozen.graph.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frozen.graph.is_empty()
    }
}

/// Parse and lower files into `graph`; returns (parse ms, lower ms).
fn lower_files(graph: &mut Graph, files: &[SourceFile]) -> (f64, f64) {
    let mut parse_ms = 0.0;
    let mut lower_ms = 0.0;
    let mut parser = sysml_syntax::parser();
    for f in files {
        let clock = Clock::start();
        let tree = parser
            .parse(f.text, None)
            .expect("parsing without a timeout always yields a tree");
        parse_ms += clock.ms();
        let clock = Clock::start();
        lower::lower_file(graph, f.path, f.text, tree.root_node());
        lower_ms += clock.ms();
    }
    (parse_ms, lower_ms)
}

/// Resolve everything in the layer being built: implied relationships
/// first (they are part of the structure), then every reference, then the
/// derived names and supertypes, so a built layer is complete.
fn finish(r: &mut Resolver) {
    // Implied relationships, to a fixpoint (a phase interrupted by a cycle
    // is retried once the elements it depends on are complete).
    for _round in 0..8 {
        let range: Vec<ElementRef> = r.g.get().refs_iter().collect();
        let mut pending = 0;
        for e in &range {
            r.ensure_implied(*e);
            if r.implied_state(*e) != 4 {
                pending += 1;
            }
        }
        if pending == 0 {
            break;
        }
    }
    for _pass in 0..2 {
        let refs: Vec<RefId> = r.g.get().ref_ids().collect();
        for id in refs {
            r.resolve_ref(id);
        }
    }
}

/// Compute and store names and supertypes of every element of the layer.
fn freeze_derived(r: &mut Resolver) {
    let all: Vec<ElementRef> = r.g.get().refs_iter().collect();
    for e in &all {
        r.name(*e);
        r.short_name(*e);
        if r.kind(*e).is_type() {
            r.supertypes(*e);
            r.supertypes_explicit(*e);
        }
    }
    // Retry references left pending by cycles.
    let refs: Vec<RefId> = r.g.get().ref_ids().collect();
    for id in refs {
        r.resolve_ref(id);
    }
}

fn unresolved_of(graph: &Graph, state: &State) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for id in graph.ref_ids() {
        let i = (id.0 - graph.ref_base) as usize;
        let resolved = matches!(state.refs.get(i), Some(RefState::Resolved(_)));
        if !resolved {
            let reference = graph.reference(id);
            let name = reference.text();
            out.push(Diagnostic {
                kind: DiagnosticKind::Unresolved,
                span: reference.span,
                message: format!("cannot resolve '{name}'"),
                name,
                reference: id,
            });
        }
    }
    out
}

/// A resolved model: the files of a project on top of a library.
pub struct Model {
    lib: Arc<Library>,
    graph: Graph,
    state: State,
    cache: RefCell<Caches>,
    project_uri: String,
    unresolved: Vec<Diagnostic>,
    stats: BuildStats,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Model")
            .field("elements", &self.graph.len())
            .field("files", &self.graph.files.len())
            .field("unresolved", &self.unresolved.len())
            .finish()
    }
}

/// The base URI used for project element ids unless one is set.
pub const DEFAULT_PROJECT_URI: &str = "urn:sysml-project:/";

impl Model {
    /// Lower and resolve `files` against `lib`.
    pub fn build(lib: Arc<Library>, files: &[SourceFile]) -> Model {
        let mut graph = Graph::above(Some(&lib.frozen.graph));
        let (parse_ms, lower_ms) = lower_files(&mut graph, files);
        Model::from_lowered(lib, graph, parse_ms, lower_ms)
    }

    fn from_lowered(lib: Arc<Library>, mut graph: Graph, parse_ms: f64, lower_ms: f64) -> Model {
        let mut state = State::default();
        let mut cache = Caches::default();
        let clock = Clock::start();
        {
            let mut r = Resolver {
                lib: Some(&lib.frozen),
                g: Access::Write(&mut graph),
                st: Access::Write(&mut state),
                cache: &mut cache,
                cuts: 0,
            };
            finish(&mut r);
            freeze_derived(&mut r);
        }
        let resolve_ms = clock.ms();
        let unresolved = unresolved_of(&graph, &state);
        let stats = BuildStats {
            parse_ms,
            lower_ms,
            resolve_ms,
            elements: graph.len(),
            references: graph.refs.len(),
        };
        Model {
            lib,
            graph,
            state,
            cache: RefCell::new(cache),
            project_uri: DEFAULT_PROJECT_URI.to_owned(),
            unresolved,
            stats,
        }
    }

    /// Set the base URI from which the ids of project elements are derived
    /// (KerML 9.1: `uuid5(NameSpace_URL, base_uri + name)`).
    pub fn set_project_uri(&mut self, uri: impl Into<String>) {
        self.project_uri = uri.into();
    }

    pub fn library(&self) -> &Arc<Library> {
        &self.lib
    }

    pub fn stats(&self) -> BuildStats {
        self.stats
    }

    fn with<R>(&self, f: impl FnOnce(&mut Resolver) -> R) -> R {
        let mut cache = self.cache.borrow_mut();
        let mut r = Resolver {
            lib: Some(&self.lib.frozen),
            g: Access::Read(&self.graph),
            st: Access::Read(&self.state),
            cache: &mut cache,
            cuts: 0,
        };
        f(&mut r)
    }

    // ------------------------------------------------------------------
    // Files and elements

    /// The project files (not library files), in the order given.
    pub fn files(&self) -> Vec<FileId> {
        self.graph.file_ids().collect()
    }

    /// Library files.
    pub fn library_files(&self) -> Vec<FileId> {
        self.lib.frozen.graph.file_ids().collect()
    }

    pub fn file_path(&self, f: FileId) -> &str {
        &self.file(f).path
    }

    fn file(&self, f: FileId) -> &crate::graph::File {
        if self.graph.file_ids().any(|x| x == f) {
            self.graph.file(f)
        } else {
            self.lib.frozen.graph.file(f)
        }
    }

    /// The file with the given path.
    pub fn file_by_path(&self, path: &str) -> Option<FileId> {
        self.graph
            .file_ids()
            .chain(self.lib.frozen.graph.file_ids())
            .find(|f| &*self.file(*f).path == path)
    }

    /// The root namespace of a file.
    pub fn file_root(&self, f: FileId) -> ElementRef {
        self.file(f).root
    }

    pub fn is_library(&self, e: ElementRef) -> bool {
        self.lib.frozen.graph.contains(e)
    }

    pub fn element(&self, e: ElementRef) -> &Element {
        if self.lib.frozen.graph.contains(e) {
            self.lib.frozen.graph.get(e)
        } else {
            self.graph.get(e)
        }
    }

    pub fn kind(&self, e: ElementRef) -> Metaclass {
        self.element(e).kind
    }

    pub fn span(&self, e: ElementRef) -> Span {
        self.element(e).span
    }

    /// All elements of the project layer.
    pub fn elements(&self) -> impl Iterator<Item = ElementRef> + '_ {
        self.graph.refs_iter()
    }

    /// All elements of the library layer.
    pub fn library_elements(&self) -> impl Iterator<Item = ElementRef> + '_ {
        self.lib.frozen.graph.refs_iter()
    }

    /// `owningRelationship` / `owningRelatedElement`.
    pub fn owner(&self, e: ElementRef) -> Option<ElementRef> {
        self.element(e).owner
    }

    /// `ownedRelationship` / `ownedRelatedElement`.
    pub fn owned(&self, e: ElementRef) -> &[ElementRef] {
        &self.element(e).owned
    }

    /// The namespace owning `e` through a membership.
    pub fn owning_namespace(&self, e: ElementRef) -> Option<ElementRef> {
        self.with(|r| r.owning_namespace(e))
    }

    /// The type owning `e` through a feature membership.
    pub fn owning_type(&self, e: ElementRef) -> Option<ElementRef> {
        self.with(|r| r.owning_type(e))
    }

    /// Effective name (`name`): declared, or via naming feature
    /// (redefinition) or the SysML rules.
    pub fn name(&self, e: ElementRef) -> Option<String> {
        self.with(|r| r.name(e)).map(String::from)
    }

    /// Effective short name.
    pub fn short_name(&self, e: ElementRef) -> Option<String> {
        self.with(|r| r.short_name(e)).map(String::from)
    }

    /// `qualifiedName` (KerML 8.3.2.1).
    pub fn qualified_name(&self, e: ElementRef) -> Option<String> {
        self.with(|r| qualified_name(r, e))
    }

    /// `elementId` (KerML 9.1): library elements get their normative ids;
    /// project elements get name-based ids under the project URI.
    pub fn element_id(&self, e: ElementRef) -> Option<ElementId> {
        self.with(|r| element_id(r, &self.lib.specs, &self.project_uri, e))
    }

    // ------------------------------------------------------------------
    // References

    /// All references (written qualified names) of the project.
    pub fn references(&self) -> impl Iterator<Item = RefId> + '_ {
        self.graph.ref_ids()
    }

    pub fn reference(&self, r: RefId) -> &Reference {
        if self.lib.frozen.graph.contains_ref(r) {
            self.lib.frozen.graph.reference(r)
        } else {
            self.graph.reference(r)
        }
    }

    /// What reference `r` resolved to.
    pub fn resolution(&self, r: RefId) -> Option<Resolution> {
        let state = if self.lib.frozen.graph.contains_ref(r) {
            self.lib
                .frozen
                .state
                .refs
                .get((r.0 - self.lib.frozen.graph.ref_base) as usize)
        } else {
            self.state.refs.get((r.0 - self.graph.ref_base) as usize)
        };
        match state {
            Some(RefState::Resolved(res)) => Some(*res),
            _ => None,
        }
    }

    /// The relationship (or expression membership) a reference belongs to.
    pub fn reference_owner(&self, r: RefId) -> ElementRef {
        self.reference(r).owner
    }

    /// The reference whose text contains `offset` in `file`, with the
    /// index of the qualified-name segment under the offset.
    pub fn reference_at(&self, file: FileId, offset: usize) -> Option<(RefId, usize)> {
        let refs: Box<dyn Iterator<Item = RefId>> = if self.graph.file_ids().any(|f| f == file) {
            Box::new(self.graph.ref_ids())
        } else {
            Box::new(self.lib.frozen.graph.ref_ids())
        };
        let mut best: Option<(RefId, usize, u32)> = None;
        for r in refs {
            let reference = self.reference(r);
            if reference.span.file() != file || !reference.span.contains(offset) {
                continue;
            }
            let seg = reference
                .segment_spans
                .iter()
                .position(|(s, e)| (*s as usize) <= offset && offset <= (*e as usize))
                .unwrap_or(reference.segments.len().saturating_sub(1));
            let len = reference.span.len();
            if best.map(|b| len < b.2).unwrap_or(true) {
                best = Some((r, seg, len));
            }
        }
        best.map(|(r, s, _)| (r, s))
    }

    /// The element the reference at `offset` denotes (for a qualified name,
    /// the element named by the segments up to the one under the offset).
    pub fn resolve_at(&self, file: FileId, offset: usize) -> Option<ElementRef> {
        let (r, seg) = self.reference_at(file, offset)?;
        let reference = self.reference(r);
        if seg + 1 >= reference.segments.len() {
            return self.resolution(r).map(|res| res.element);
        }
        // A prefix of the qualified name: resolve it in the same context.
        let full = self.resolution(r)?;
        let mut cur = Some(full.element);
        // Walk up the owning namespaces of the full target as long as the
        // names match; otherwise resolve the prefix explicitly.
        let prefix: Vec<Box<str>> = reference.segments[..=seg].to_vec();
        let owner = reference.owner;
        let by_owners = {
            let mut steps = reference.segments.len() - 1 - seg;
            while steps > 0 {
                cur = cur.and_then(|c| self.owning_namespace(c));
                steps -= 1;
            }
            cur
        };
        let explicit = self.with(|r| {
            let ns = r.local_namespace(owner)?;
            r.resolve_qualified(ns, &prefix).map(|res| res.element)
        });
        explicit.or(by_owners)
    }

    /// Diagnostics for unresolved references in the project files.
    pub fn unresolved(&self) -> &[Diagnostic] {
        &self.unresolved
    }

    /// All diagnostics of the semantic model.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.unresolved
    }

    // ------------------------------------------------------------------
    // Position queries

    /// The innermost element (not relationship) of `file` whose source
    /// span contains `offset`.
    pub fn element_at(&self, file: FileId, offset: usize) -> Option<ElementRef> {
        let graph = if self.graph.file_ids().any(|f| f == file) {
            &self.graph
        } else {
            &self.lib.frozen.graph
        };
        let root = self.file(file).root;
        let mut cur = root;
        'descend: loop {
            for rel in &graph.get(cur).owned {
                let rel_el = graph.get(*rel);
                if rel_el.is_implied() {
                    continue;
                }
                for child in &rel_el.owned {
                    let c = graph.get(*child);
                    if c.span.file() == file && c.span.contains(offset) && !c.kind.is_relationship()
                    {
                        cur = *child;
                        continue 'descend;
                    }
                }
            }
            return Some(cur);
        }
    }

    // ------------------------------------------------------------------
    // Namespaces and members

    /// Owned memberships of a namespace.
    pub fn owned_memberships(&self, ns: ElementRef) -> Vec<ElementRef> {
        self.owned(ns)
            .iter()
            .copied()
            .filter(|m| self.kind(*m).is_membership())
            .collect()
    }

    /// Owned members (elements owned through memberships).
    pub fn owned_members(&self, ns: ElementRef) -> Vec<ElementRef> {
        self.with(|r| r.owned_members(ns))
    }

    /// All memberships of a namespace: owned, imported and (for a type)
    /// inherited, any visibility — what local name resolution sees.
    pub fn members(&self, ns: ElementRef) -> Vec<Member> {
        self.table(TableKey::All(ns))
    }

    /// Publicly visible memberships (what `ns::name` can resolve to).
    pub fn visible_members(&self, ns: ElementRef) -> Vec<Member> {
        self.table(TableKey::Visible(ns, false, false))
    }

    /// Inherited memberships of a type (redefined features removed).
    pub fn inherited_members(&self, t: ElementRef) -> Vec<Member> {
        self.table(TableKey::Inherited(t, false))
    }

    /// Imported memberships of a namespace.
    pub fn imported_members(&self, ns: ElementRef) -> Vec<Member> {
        self.members(ns)
            .into_iter()
            .filter(|m| m.origin == crate::resolve::Origin::Imported)
            .collect()
    }

    fn table(&self, key: TableKey) -> Vec<Member> {
        self.with(|r| r.table(key))
            .map(|t| t.entries.iter().map(Member::from).collect())
            .unwrap_or_default()
    }

    /// Resolve a qualified name with `ns` as the local namespace (basic
    /// resolution, KerML 8.2.3.5.1). `$::` resolves globally.
    pub fn resolve_name(&self, ns: ElementRef, qualified_name: &str) -> Option<ElementRef> {
        let (global, rest) = match qualified_name.strip_prefix("$::") {
            Some(rest) => (true, rest),
            None => (false, qualified_name),
        };
        let segments = crate::resolve::split_qualified_name(rest);
        self.with(|r| {
            if global {
                r.library_element(rest)
            } else {
                r.resolve_qualified(ns, &segments).map(|res| res.element)
            }
        })
    }

    /// Resolve a qualified name in the global namespace.
    pub fn resolve_global(&self, qualified_name: &str) -> Option<ElementRef> {
        self.with(|r| r.library_element(qualified_name))
    }

    // ------------------------------------------------------------------
    // Types and features

    /// Direct supertypes (general types of owned specializations, explicit
    /// and implied; the original type of a conjugated type).
    pub fn supertypes(&self, t: ElementRef) -> Vec<ElementRef> {
        self.with(|r| r.supertypes(t).to_vec())
    }

    /// All direct and indirect supertypes.
    pub fn all_supertypes(&self, t: ElementRef) -> Vec<ElementRef> {
        let mut v: Vec<ElementRef> = self.with(|r| r.all_supertypes(t).iter().copied().collect());
        v.sort();
        v
    }

    pub fn specializes(&self, t: ElementRef, general: ElementRef) -> bool {
        self.with(|r| r.specializes(t, general))
    }

    /// `type` of a feature: the types of its typings, and of the features
    /// it subsets or redefines (and of the last feature of a chain),
    /// transitively (KerML `Feature::type`, without the implied library
    /// classifiers).
    pub fn types(&self, feature: ElementRef) -> Vec<ElementRef> {
        self.with(|r| {
            let mut out = Vec::new();
            let mut seen = std::collections::HashSet::new();
            let mut stack = vec![feature];
            while let Some(f) = stack.pop() {
                if !seen.insert(f) {
                    continue;
                }
                let rels: Vec<ElementRef> = r.el(f).owned.clone();
                for rel in rels {
                    let k = r.kind(rel);
                    if k.is_feature_typing() {
                        if let Some(t) = r.general(rel) {
                            if !out.contains(&t) {
                                out.push(t);
                            }
                        }
                    } else if k.is_subsetting() && k != M::CrossSubsetting {
                        if let Some(g) = r.general(rel) {
                            stack.push(g);
                        }
                    } else if k == M::FeatureChaining {
                        // handled below
                    }
                }
                let chainings = r.owned_rels(f, |k| k == M::FeatureChaining);
                if let Some(last) = chainings.last() {
                    if let Some(g) = r.general(*last) {
                        stack.push(g);
                    }
                }
            }
            out
        })
    }

    /// Features owned by a type (through feature memberships).
    pub fn owned_features(&self, t: ElementRef) -> Vec<ElementRef> {
        self.owned(t)
            .iter()
            .copied()
            .filter(|m| self.kind(*m).is_feature_membership())
            .filter_map(|m| {
                let el = self.element(m);
                el.owned.first().copied()
            })
            .collect()
    }

    /// Features of a type: owned and inherited.
    pub fn features(&self, t: ElementRef) -> Vec<ElementRef> {
        self.members(t)
            .into_iter()
            .filter(|m| self.kind(m.membership).is_feature_membership())
            .filter(|m| m.origin != crate::resolve::Origin::Imported)
            .map(|m| m.element)
            .collect()
    }

    /// The owned relationships of an element (explicit then implied).
    pub fn relationships(&self, e: ElementRef) -> Vec<ElementRef> {
        self.owned(e)
            .iter()
            .copied()
            .filter(|r| self.kind(*r).is_relationship())
            .collect()
    }

    /// The resolved non-owning ends of a relationship (target, and source
    /// for standalone relationships).
    pub fn relationship_ends(&self, rel: ElementRef) -> Vec<(Role, Option<ElementRef>)> {
        let ends = self.element(rel).ends.clone();
        ends.iter()
            .map(|end| {
                let t = match end.target {
                    Target::Element(e) | Target::Owned(e) => Some(e),
                    Target::Name(r) => self.resolution(r).map(|res| res.element),
                };
                (end.role, t)
            })
            .collect()
    }

    /// The target of a relationship (`general`, `type`, `memberElement`,
    /// `importedNamespace`, ...). For an owned feature chain target, the
    /// chain feature.
    pub fn relationship_target(&self, rel: ElementRef) -> Option<ElementRef> {
        self.relationship_ends(rel)
            .into_iter()
            .find(|(role, _)| *role == Role::Target)
            .and_then(|(_, t)| t)
    }

    /// `featureTarget`: the last chaining feature of a chain, or itself.
    pub fn feature_target(&self, f: ElementRef) -> ElementRef {
        self.with(|r| r.feature_target(f))
    }

    /// Whether `e` was added by the semantic model (implied relationship).
    pub fn is_implied(&self, e: ElementRef) -> bool {
        self.element(e).flags.contains(Flags::IMPLIED)
    }

    /// The library element with the given qualified name.
    pub fn library_element(&self, qualified_name: &str) -> Option<ElementRef> {
        self.with(|r| r.library_element(qualified_name))
    }
}

/// `qualifiedName` (KerML 8.3.2.1).
pub(crate) fn qualified_name(r: &mut Resolver, e: ElementRef) -> Option<String> {
    let ns = r.owning_namespace(e)?;
    let escaped = escaped_name(r, e);
    let first = match r.name(e) {
        None => true,
        Some(n) => {
            let members = r.owned_members(ns);
            let mut first = None;
            for m in members {
                if r.name(m).as_deref() == Some(&*n) {
                    first = Some(m);
                    break;
                }
            }
            first == Some(e)
        }
    };
    if r.owner(ns).is_none() {
        return name::qualified_name(OwningNamespace::Root, escaped.as_deref(), first);
    }
    let ns_qn = qualified_name(r, ns);
    name::qualified_name(
        OwningNamespace::Nested(ns_qn.as_deref()),
        escaped.as_deref(),
        first,
    )
}

fn escaped_name(r: &mut Resolver, e: ElementRef) -> Option<String> {
    r.name(e)
        .or_else(|| r.short_name(e))
        .map(|n| name::escape_name(&n).into_owned())
}

struct Tree<'r, 'a> {
    r: RefCell<&'r mut Resolver<'a>>,
}

impl OwnershipTree for Tree<'_, '_> {
    type Node = ElementRef;

    fn owning_relationship(&self, node: ElementRef) -> Option<ElementRef> {
        let r = self.r.borrow();
        if r.kind(node).is_relationship() {
            return None;
        }
        r.owner(node)
    }

    fn owned_related_element_index(&self, node: ElementRef) -> usize {
        let r = self.r.borrow();
        let owner = r.owner(node).unwrap();
        1 + r
            .el(owner)
            .owned
            .iter()
            .position(|x| *x == node)
            .unwrap_or(0)
    }

    fn owning_related_element(&self, node: ElementRef) -> Option<ElementRef> {
        let r = self.r.borrow();
        if !r.kind(node).is_relationship() {
            return None;
        }
        r.owner(node)
    }

    fn owned_relationship_index(&self, node: ElementRef) -> usize {
        let r = self.r.borrow();
        let owner = r.owner(node).unwrap();
        1 + r
            .el(owner)
            .owned
            .iter()
            .position(|x| *x == node)
            .unwrap_or(0)
    }

    fn is_relationship(&self, node: ElementRef) -> bool {
        self.r.borrow().kind(node).is_relationship()
    }

    fn owned_member_element(&self, node: ElementRef) -> Option<ElementRef> {
        self.r.borrow().owned_member(node)
    }

    fn qualified_name(&self, node: ElementRef) -> Option<String> {
        let mut r = self.r.borrow_mut();
        if r.kind(node).is_relationship() {
            return None;
        }
        qualified_name(&mut r, node)
    }
}

/// Element id (KerML 9.1).
pub(crate) fn element_id(
    r: &mut Resolver,
    specs: &[Spec],
    project_uri: &str,
    e: ElementRef,
) -> Option<ElementId> {
    // Find the root namespace and the top-level element containing `e`.
    let mut top = e;
    let mut chain = vec![e];
    while let Some(o) = r.owner(top) {
        top = o;
        chain.push(o);
    }
    let root = top;
    let lib = r.is_lib(e);
    let file_index = {
        let files: Vec<(ElementRef, usize)> = if lib {
            r.lib
                .map(|l| {
                    l.graph
                        .files
                        .iter()
                        .enumerate()
                        .map(|(i, f)| (f.root, i))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            r.g.get()
                .files
                .iter()
                .enumerate()
                .map(|(i, f)| (f.root, i))
                .collect()
        };
        files
            .iter()
            .find(|(root_el, _)| *root_el == root)
            .map(|(_, i)| *i)?
    };
    let path_str = if lib {
        r.lib.unwrap().graph.files[file_index].path.to_string()
    } else {
        r.g.get().files[file_index].path.to_string()
    };
    if e == root {
        return Some(ElementId::top_level(project_uri, &path_str));
    }
    // The top-level element: the member of the root's owning membership.
    // chain = [e, ..., member_of_root?, membership, root]
    let n = chain.len();
    let top_element = if n >= 3 { Some(chain[n - 3]) } else { None };
    let top_name = top_element.and_then(|t| r.name(t));
    let base_id = match (top_element, top_name) {
        (Some(t), Some(name)) if !r.kind(t).is_relationship() => {
            if lib {
                ElementId::library_package(
                    specs.get(file_index).copied().unwrap_or(Spec::SysML),
                    &name,
                )
            } else {
                ElementId::top_level(project_uri, &name)
            }
        }
        _ => ElementId::top_level(project_uri, &path_str),
    };
    if Some(e) == top_element && top_element.and_then(|t| r.name(t)).is_some() {
        return Some(base_id);
    }
    let tree = Tree { r: RefCell::new(r) };
    let path = id::path(&tree, e);
    Some(ElementId::contained(base_id, &path))
}

/// Unused-slot helper kept for the frozen state API.
#[allow(dead_code)]
fn slot_done<T: Clone>(slot: &Slot<T>) -> Option<T> {
    match slot {
        Slot::Done(v) => Some(v.clone()),
        Slot::Pending => None,
    }
}

/// A set of project files with cached parse trees: editing one file
/// re-parses only that file and rebuilds the model.
pub struct Project {
    lib: Arc<Library>,
    files: Vec<(String, String, sysml_syntax::tree_sitter::Tree)>,
    parser: sysml_syntax::tree_sitter::Parser,
}

impl Project {
    pub fn new(lib: Arc<Library>) -> Project {
        Project {
            lib,
            files: Vec::new(),
            parser: sysml_syntax::parser(),
        }
    }

    /// Add or replace a file.
    pub fn set_file(&mut self, path: &str, text: &str) {
        let tree = self
            .parser
            .parse(text, None)
            .expect("parsing without a timeout always yields a tree");
        match self.files.iter_mut().find(|(p, _, _)| p == path) {
            Some(slot) => {
                slot.1 = text.to_owned();
                slot.2 = tree;
            }
            None => self.files.push((path.to_owned(), text.to_owned(), tree)),
        }
    }

    pub fn remove_file(&mut self, path: &str) {
        self.files.retain(|(p, _, _)| p != path);
    }

    /// Lower and resolve the current files (parse trees are reused).
    pub fn build(&self) -> Model {
        let mut graph = Graph::above(Some(&self.lib.frozen.graph));
        let clock = Clock::start();
        for (path, text, tree) in &self.files {
            lower::lower_file(&mut graph, path, text, tree.root_node());
        }
        let lower_ms = clock.ms();
        Model::from_lowered(self.lib.clone(), graph, 0.0, lower_ms)
    }
}

#[allow(dead_code)]
fn _assert_send() {
    fn is_send<T: Send>() {}
    is_send::<Model>();
    fn is_sync<T: Send + Sync>() {}
    is_sync::<Library>();
    let _: HashMap<u8, u8> = HashMap::new();
}

impl Model {
    /// A textual dump of the element tree under `e` (kinds, names, flags,
    /// relationship targets), for debugging and tests.
    pub fn dump(&self, e: ElementRef) -> String {
        let mut out = String::new();
        self.dump_into(e, 0, &mut out);
        out
    }

    fn dump_into(&self, e: ElementRef, depth: usize, out: &mut String) {
        let el = self.element(e);
        let mut line = format!("{}{}", "  ".repeat(depth), el.kind.name());
        if let Some(n) = &el.name {
            line.push_str(&format!(" '{n}'"));
        } else if let Some(n) = self.name(e) {
            line.push_str(&format!(" ('{n}')"));
        }
        if let Some(s) = &el.short_name {
            line.push_str(&format!(" <{s}>"));
        }
        if el.flags != Flags::NONE {
            line.push_str(&format!(" [{:?}]", el.flags));
        }
        if let Some(d) = el.direction {
            line.push_str(&format!(" {}", d.name()));
        }
        for end in &el.ends {
            let (text, target) = match end.target {
                Target::Name(r) => (
                    self.reference(r).text(),
                    self.resolution(r).map(|res| res.element),
                ),
                Target::Element(t) => (String::from("<implied>"), Some(t)),
                Target::Owned(t) => (String::from("<owned>"), Some(t)),
            };
            let target = match target {
                Some(t) => self
                    .qualified_name(t)
                    .unwrap_or_else(|| format!("{:?}:{}", t, self.kind(t).name())),
                None => String::from("UNRESOLVED"),
            };
            line.push_str(&format!(" {:?}={text}->{target}", end.role));
        }
        out.push_str(&line);
        out.push('\n');
        for c in &el.owned {
            self.dump_into(*c, depth + 1, out);
        }
    }
}
