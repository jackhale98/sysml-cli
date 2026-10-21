//! Name resolution (KerML 8.2.3.5) and the derived properties it depends on:
//! effective names, memberships (owned, imported, inherited, visible),
//! supertypes (explicit and implied), and the resolution of every written
//! qualified name and feature chain.
//!
//! Resolution is demand driven, as the spec requires ("name resolution must
//! proceed incrementally ... avoiding infinite loops caused by attempting to
//! resolve again names that are already pending resolution", 8.2.3.5.1):
//! each derived value has a pending/in-progress/done state, re-entering an
//! in-progress computation yields "nothing" and *taints* the computations on
//! the stack, and tainted results are not cached (they are recomputed when
//! asked again outside the cycle).
//!
//! A model has two layers (see [`crate::graph`]): the standard library,
//! whose derived values are computed once and frozen in [`Frozen`], and a
//! project layer resolved against it. Library membership tables that the
//! library build did not need are computed on demand into the project's
//! overlay cache.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::graph::Element;
use crate::graph::{ElementRef, End, Flags, Graph, RefId, Role, Target, Visibility};
use crate::metaclass::Metaclass as M;
use crate::metaclass::Metaclass;

/// The result of resolving a qualified name: the membership it names and
/// that membership's member element.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Resolution {
    pub membership: ElementRef,
    pub element: ElementRef,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum RefState {
    #[default]
    Pending,
    InProgress,
    Resolved(Resolution),
    Failed,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) enum Slot<T> {
    #[default]
    Pending,
    Done(T),
}

/// Where a membership in a table comes from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Origin {
    Owned,
    Imported,
    Inherited,
}

/// One membership of a namespace, as seen from that namespace.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Entry {
    pub membership: ElementRef,
    pub element: ElementRef,
    /// Visibility relative to the namespace (an imported membership has
    /// the visibility of its import).
    pub visibility: Visibility,
    pub origin: Origin,
}

/// A list of memberships with a name index.
#[derive(Clone, Debug, Default)]
pub struct Table {
    pub entries: Vec<Entry>,
    index: HashMap<Box<str>, Vec<u32>>,
}

impl Table {
    pub fn lookup(&self, name: &str) -> Option<&Entry> {
        self.index
            .get(name)
            .and_then(|v| v.first())
            .map(|i| &self.entries[*i as usize])
    }

    pub fn lookup_all(&self, name: &str) -> impl Iterator<Item = &Entry> {
        self.index
            .get(name)
            .into_iter()
            .flatten()
            .map(|i| &self.entries[*i as usize])
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum TableKey {
    /// All memberships (owned, imported, inherited) for local resolution.
    All(ElementRef),
    /// `visibleMemberships(isRecursive, includeAll)`.
    Visible(ElementRef, bool, bool),
    /// `nonPrivateMemberships`, inherited by specializations.
    NonPrivate(ElementRef, bool),
    /// Inherited memberships (excluding implied supertypes if the flag is set).
    Inherited(ElementRef, bool),
    /// Owned, imported and inherited memberships *including* features
    /// that are redefined further down (for resolving redefined features).
    Redefinable(ElementRef),
}

/// Derived per-element and per-reference values of one layer.
#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    pub refs: Vec<RefState>,
    pub names: Vec<Slot<Option<Box<str>>>>,
    pub short_names: Vec<Slot<Option<Box<str>>>>,
    pub supers: Vec<Slot<Arc<[ElementRef]>>>,
    pub supers_explicit: Vec<Slot<Arc<[ElementRef]>>>,
    /// 0 = implied relationships not computed, 1 = in progress, 2 = done.
    pub implied: Vec<u8>,
}

/// A frozen, fully resolved layer (the standard library).
#[derive(Debug, Default)]
pub(crate) struct Frozen {
    pub graph: Graph,
    pub state: State,
    pub tables: HashMap<TableKey, Arc<Table>>,
    pub all_supertypes: HashMap<ElementRef, Arc<HashSet<ElementRef>>>,
    pub lib_cache: HashMap<String, Option<ElementRef>>,
}

/// Mutable caches of the layer being resolved.
#[derive(Debug, Default)]
pub(crate) struct Caches {
    pub tables: HashMap<TableKey, Arc<Table>>,
    pub tables_in_progress: HashSet<TableKey>,
    pub all_supertypes: HashMap<ElementRef, Arc<HashSet<ElementRef>>>,
    pub redefined: HashMap<ElementRef, Arc<[ElementRef]>>,
    pub lib_cache: HashMap<String, Option<ElementRef>>,
    pub global: HashMap<Box<str>, Vec<Entry>>,
    pub global_built: bool,
    pub names_busy: HashSet<ElementRef>,
    pub short_names_busy: HashSet<ElementRef>,
    pub supers_busy: HashSet<(ElementRef, bool)>,
}

/// Write access to the layer being resolved (while building) or read-only
/// access (queries on a built model: derived values are then recomputed
/// rather than cached, and no implied relationships are added).
pub(crate) enum Access<'a, T> {
    Write(&'a mut T),
    Read(&'a T),
}

impl<T> Access<'_, T> {
    pub fn get(&self) -> &T {
        match self {
            Access::Write(t) => t,
            Access::Read(t) => t,
        }
    }
    pub fn get_mut(&mut self) -> Option<&mut T> {
        match self {
            Access::Write(t) => Some(t),
            Access::Read(_) => None,
        }
    }
}

/// The resolution context: an optional frozen lower layer and the layer
/// being resolved.
pub(crate) struct Resolver<'a> {
    pub lib: Option<&'a Frozen>,
    pub g: Access<'a, Graph>,
    pub st: Access<'a, State>,
    pub cache: &'a mut Caches,
    /// Number of cycle cuts so far; a computation that saw the counter
    /// change is tainted and not cached.
    pub cuts: u64,
}

fn ensure<T: Default>(v: &mut Vec<T>, i: usize) {
    if v.len() <= i {
        v.resize_with(i + 1, T::default);
    }
}

impl<'a> Resolver<'a> {
    // ------------------------------------------------------------------
    // Layer access

    pub fn el(&self, e: ElementRef) -> &Element {
        match self.lib {
            Some(l) if l.graph.contains(e) => l.graph.get(e),
            _ => self.g.get().get(e),
        }
    }

    pub fn is_lib(&self, e: ElementRef) -> bool {
        matches!(self.lib, Some(l) if l.graph.contains(e))
    }

    pub fn kind(&self, e: ElementRef) -> Metaclass {
        self.el(e).kind
    }

    fn local_index(&self, e: ElementRef) -> usize {
        (e.0 - self.g.get().base) as usize
    }

    fn local_ref_index(&self, r: RefId) -> usize {
        (r.0 - self.g.get().ref_base) as usize
    }

    pub fn reference(&self, r: RefId) -> &crate::graph::Reference {
        match self.lib {
            Some(l) if l.graph.contains_ref(r) => l.graph.reference(r),
            _ => self.g.get().reference(r),
        }
    }

    /// `owningRelationship` / `owningRelatedElement`.
    pub fn owner(&self, e: ElementRef) -> Option<ElementRef> {
        self.el(e).owner
    }

    /// `owningNamespace`: the namespace owning the membership that owns `e`.
    pub fn owning_namespace(&self, e: ElementRef) -> Option<ElementRef> {
        let rel = self.owner(e)?;
        if !self.kind(rel).is_membership() {
            return None;
        }
        self.owner(rel)
    }

    /// The owning membership of `e`, if `e` is owned by one.
    pub fn owning_membership(&self, e: ElementRef) -> Option<ElementRef> {
        let rel = self.owner(e)?;
        self.kind(rel).is_owning_membership().then_some(rel)
    }

    /// `owningType` (for a feature owned through a FeatureMembership) or,
    /// for a relationship, its owning related element if that is a type.
    pub fn owning_type(&self, e: ElementRef) -> Option<ElementRef> {
        let rel = self.owner(e)?;
        if !self.kind(rel).is_feature_membership() {
            return None;
        }
        let t = self.owner(rel)?;
        self.kind(t).is_type().then_some(t)
    }

    /// The ownedRelationships of `e` of kinds matching `pred`.
    pub fn owned_rels(&self, e: ElementRef, pred: impl Fn(Metaclass) -> bool) -> Vec<ElementRef> {
        self.el(e)
            .owned
            .iter()
            .copied()
            .filter(|r| pred(self.kind(*r)))
            .collect()
    }

    /// The single owned related element of a relationship (the member of
    /// an owning membership).
    pub fn owned_member(&self, m: ElementRef) -> Option<ElementRef> {
        if !self.kind(m).is_owning_membership() {
            return None;
        }
        self.el(m).owned.first().copied()
    }

    /// Owned members (elements of owning memberships) of a namespace.
    pub fn owned_members(&self, ns: ElementRef) -> Vec<ElementRef> {
        self.el(ns)
            .owned
            .iter()
            .filter_map(|m| self.owned_member(*m))
            .collect()
    }

    // ------------------------------------------------------------------
    // Relationship targets

    /// The element at the end `role` of relationship `rel`, resolving its
    /// qualified name if necessary.
    pub fn end_target(&mut self, rel: ElementRef, role: Role) -> Option<ElementRef> {
        let target = self.el(rel).end(role)?;
        self.target_element(target)
    }

    pub fn target_element(&mut self, target: Target) -> Option<ElementRef> {
        match target {
            Target::Element(e) | Target::Owned(e) => Some(e),
            Target::Name(r) => self.resolve_ref(r).map(|res| res.element),
        }
    }

    /// The general/target type of a specialization relationship.
    pub fn general(&mut self, rel: ElementRef) -> Option<ElementRef> {
        self.end_target(rel, Role::Target)
    }

    // ------------------------------------------------------------------
    // Reference resolution

    pub fn ref_state(&self, r: RefId) -> RefState {
        match self.lib {
            Some(l) if l.graph.contains_ref(r) => l
                .state
                .refs
                .get((r.0 - l.graph.ref_base) as usize)
                .copied()
                .unwrap_or_default(),
            _ => self
                .st
                .get()
                .refs
                .get(self.local_ref_index(r))
                .copied()
                .unwrap_or_default(),
        }
    }

    fn set_ref_state(&mut self, r: RefId, s: RefState) {
        let i = self.local_ref_index(r);
        if let Some(st) = self.st.get_mut() {
            ensure(&mut st.refs, i);
            st.refs[i] = s;
        }
    }

    /// Resolve the reference `r` (memoized).
    pub fn resolve_ref(&mut self, r: RefId) -> Option<Resolution> {
        match self.ref_state(r) {
            RefState::Resolved(res) => return Some(res),
            RefState::Failed => return None,
            RefState::InProgress => {
                self.cuts += 1;
                return None;
            }
            RefState::Pending => {}
        }
        if self.lib.is_some_and(|l| l.graph.contains_ref(r)) {
            return None;
        }
        self.set_ref_state(r, RefState::InProgress);
        let cuts = self.cuts;
        let result = self.resolve_ref_uncached(r);
        let state = match result {
            Some(res) => RefState::Resolved(res),
            None if self.cuts == cuts => RefState::Failed,
            None => RefState::Pending,
        };
        self.set_ref_state(r, state);
        result
    }

    fn resolve_ref_uncached(&mut self, r: RefId) -> Option<Resolution> {
        let reference = self.reference(r).clone();
        let rel = reference.owner;
        let rel_kind = self.kind(rel);
        let segments = &reference.segments;
        if segments.is_empty() {
            return None;
        }

        // FeatureChaining: after the first, resolve in the previous chaining feature.
        if rel_kind == M::FeatureChaining {
            let chain = self.owner(rel)?;
            let chainings = self.owned_rels(chain, |k| k == M::FeatureChaining);
            let pos = chainings.iter().position(|c| *c == rel)?;
            if pos > 0 {
                let prev = self.general(chainings[pos - 1])?;
                return self.resolve_in_feature(prev, segments, reference.global);
            }
            // First chaining: as if the chain's owning relationship were the context.
            let ctx = self.owner(chain)?;
            return self.resolve_in_context(ctx, chain, segments, reference.global, true);
        }
        let res = self.resolve_in_context(rel, rel, segments, reference.global, false)?;
        // Post-processing by context.
        match rel_kind {
            M::ConjugatedPortTyping => {
                let conj = self.conjugated_port_definition(res.element)?;
                Some(Resolution {
                    membership: self.owner(conj).unwrap_or(res.membership),
                    element: conj,
                })
            }
            _ => Some(res),
        }
    }

    /// Resolve `segments` written as the target of relationship `ctx`
    /// (`subject` is the element whose owning relationship `ctx` is, for
    /// chains; otherwise `ctx` itself).
    fn resolve_in_context(
        &mut self,
        ctx: ElementRef,
        subject: ElementRef,
        segments: &[Box<str>],
        global: bool,
        for_chain: bool,
    ) -> Option<Resolution> {
        if global {
            return self.resolve_qualified_from_global(segments);
        }
        let kind = self.kind(ctx);
        let _ = subject;
        let _ = for_chain;

        // Redefinition with an owning feature that has an owning type:
        // resolve in each general type of the owning type (8.2.3.5.1).
        // Features the general itself redefines stay visible for this
        // purpose (an end of `BinaryLink` can be redefined again in a
        // specialization of a link that already redefined it), as in the
        // reference implementation. When the generals do not have the
        // name, resolution falls back to the ordinary rules (the library
        // relies on this, e.g. `FeatureMonitorPerformance`).
        if kind == M::Redefinition {
            if let Some(feature) = self.owner(ctx) {
                if self.kind(feature).is_feature() {
                    if let Some(owning_type) = self.owning_type_or_featuring(feature) {
                        let generals = self.redefinition_scopes(owning_type);
                        if segments.len() == 1 {
                            for g in &generals {
                                let g = self.namespace_for_lookup(*g);
                                if let Some(res) = self.resolve_in_redefinable(g, &segments[0]) {
                                    return Some(res);
                                }
                            }
                        }
                        if let Some(ns) = self.local_namespace(ctx) {
                            if let Some(res) = self.resolve_qualified(ns, segments) {
                                return Some(res);
                            }
                        }
                        for g in &generals {
                            if let Some(res) = self.resolve_qualified(*g, segments) {
                                return Some(res);
                            }
                        }
                        return None;
                    }
                }
            }
        }

        // Feature chain expressions: resolve in the result of the argument.
        if kind == M::Membership || kind == M::OwningMembership {
            if let Some(owner) = self.owner(ctx) {
                if self.kind(owner) == M::FeatureChainExpression {
                    let source = self.chain_expression_source(owner)?;
                    return self.resolve_in_feature(source, segments, false);
                }
            }
        }

        let ns = self.local_namespace(ctx)?;
        let res = self.resolve_qualified(ns, segments)?;
        if kind.is_import() && !kind.is_namespace_import() {
            // MembershipImport: the membership itself (already in `res`).
            return Some(res);
        }
        Some(res)
    }

    /// The local namespace for a qualified name that is the target of
    /// relationship `rel` (8.2.3.5.2).
    pub fn local_namespace(&self, rel: ElementRef) -> Option<ElementRef> {
        let kind = self.kind(rel);
        let owner = self.owner(rel)?;
        if kind.is_import() {
            // A FilterPackage's import resolves in the namespace owning the
            // filtered import.
            if self.kind(owner) == M::Package {
                if let Some(outer) = self.owner(owner) {
                    if self.kind(outer).is_import() {
                        return self.owner(outer);
                    }
                }
            }
            return Some(owner);
        }
        if kind.is_membership() {
            let ok = self.kind(owner);
            if ok == M::FeatureReferenceExpression
                || (!kind.is_feature_membership() && is_instantiation(ok))
            {
                return Some(self.non_invocation_namespace(owner));
            }
            return Some(owner);
        }
        if kind.is_specialization()
            || matches!(
                kind,
                M::Conjugation
                    | M::Disjoining
                    | M::Unioning
                    | M::Intersecting
                    | M::Differencing
                    | M::FeatureInverting
                    | M::TypeFeaturing
            )
        {
            // ReferenceSubsetting of a connector end: the connector's
            // owning namespace.
            if kind == M::ReferenceSubsetting && self.el(owner).flags.contains(Flags::END) {
                if let Some(connector) = self.owning_type(owner) {
                    if self.kind(connector).is_connector() {
                        return self.namespace_of(connector);
                    }
                }
            }
            if self.kind(owner).is_type() {
                // A standalone relationship (`specialization S subtype A
                // :> B`) is owned by a membership, not by its type.
                return self.namespace_of(owner);
            }
            return self.namespace_of(rel);
        }
        if matches!(kind, M::Annotation | M::Dependency) {
            // `comment about x`, `metadata m about x`, dependency ends.
            let annotating = owner;
            if kind == M::Dependency {
                return self.namespace_of(rel);
            }
            return self.namespace_of(annotating);
        }
        self.namespace_of(rel)
    }

    /// The namespace in which names written in element `e` (not a member
    /// itself) are resolved: its owning namespace, or for an element owned
    /// by a relationship that is not a membership, the namespace of that
    /// relationship's owner.
    pub fn namespace_of(&self, e: ElementRef) -> Option<ElementRef> {
        let mut cur = e;
        loop {
            let rel = self.owner(cur)?;
            if self.kind(rel).is_membership() {
                let ns = self.owner(rel)?;
                if self.kind(ns) == M::FeatureReferenceExpression || is_instantiation(self.kind(ns))
                {
                    return Some(self.non_invocation_namespace(ns));
                }
                return Some(ns);
            }
            // Owned by a non-membership relationship (chain feature in a
            // subsetting, implied element): continue from its owner.
            cur = self.owner(rel)?;
            if !self.kind(cur).is_relationship() && cur != e {
                // `cur` is the element owning the relationship: names in
                // the relationship's owned element resolve like names of
                // the relationship itself, i.e. in cur's namespace.
                return self.namespace_of(cur);
            }
        }
    }

    /// The nearest containing namespace that is not a feature reference
    /// expression, an instantiation expression, or an owned feature of one
    /// (8.2.3.5.2).
    pub fn non_invocation_namespace(&self, ns: ElementRef) -> ElementRef {
        let mut cur = ns;
        loop {
            let k = self.kind(cur);
            let skip = k == M::FeatureReferenceExpression
                || is_instantiation(k)
                || self
                    .owning_namespace_any(cur)
                    .map(|o| is_instantiation(self.kind(o)))
                    .unwrap_or(false)
                || self
                    .owning_namespace_any(cur)
                    .and_then(|o| self.owning_namespace_any(o))
                    .map(|oo| self.kind(oo) == M::ConstructorExpression)
                    .unwrap_or(false)
                    && self.owner(cur).map(|m| self.kind(m)) == Some(M::ParameterMembership);
            if !skip {
                return cur;
            }
            match self.owning_namespace_any(cur) {
                Some(o) => cur = o,
                None => return cur,
            }
        }
    }

    /// Owning namespace, also through FeatureValue (a membership) — i.e.
    /// the element owning the membership that owns `e`.
    fn owning_namespace_any(&self, e: ElementRef) -> Option<ElementRef> {
        let rel = self.owner(e)?;
        if self.kind(rel).is_relationship() {
            self.owner(rel)
        } else {
            None
        }
    }

    /// For redefinitions: the owning type of a feature, or (for features
    /// owned through a plain OwningMembership in a type, e.g. metadata body
    /// features) the owning namespace when it is a type.
    fn owning_type_or_featuring(&self, feature: ElementRef) -> Option<ElementRef> {
        if let Some(t) = self.owning_type(feature) {
            return Some(t);
        }
        None
    }

    /// The general types of the ownedSpecializations of `t` (explicit and
    /// implied), used as scopes for resolving redefined features.
    fn redefinition_scopes(&mut self, t: ElementRef) -> Vec<ElementRef> {
        let mut out = Vec::new();
        for s in self.supertypes(t).iter() {
            if !out.contains(s) {
                out.push(*s);
            }
        }
        out
    }

    /// The feature whose members a feature chain expression's member is
    /// resolved in: the target of its argument expression.
    fn chain_expression_source(&mut self, fce: ElementRef) -> Option<ElementRef> {
        let arg_pm = self
            .el(fce)
            .owned
            .iter()
            .copied()
            .find(|m| self.kind(*m) == M::ParameterMembership)?;
        let arg = self.owned_member(arg_pm)?;
        let fv = self
            .el(arg)
            .owned
            .iter()
            .copied()
            .find(|m| self.kind(*m) == M::FeatureValue)?;
        let expr = self.owned_member(fv)?;
        self.expression_target(expr)
    }

    /// The feature (or type) that an expression evaluates to, as far as
    /// name resolution through `.` is concerned.
    pub fn expression_target(&mut self, expr: ElementRef) -> Option<ElementRef> {
        match self.kind(expr) {
            M::FeatureReferenceExpression => {
                let m = self.el(expr).owned.first().copied()?;
                match self.kind(m) {
                    M::Membership => self.end_target(m, Role::Target),
                    // Body expression.
                    _ => self.owned_member(m),
                }
            }
            M::FeatureChainExpression => {
                let member = self
                    .el(expr)
                    .owned
                    .iter()
                    .copied()
                    .find(|m| matches!(self.kind(*m), M::Membership | M::OwningMembership))?;
                if self.kind(member) == M::Membership {
                    self.end_target(member, Role::Target)
                } else {
                    let chain = self.owned_member(member)?;
                    Some(self.feature_target(chain))
                }
            }
            M::OperatorExpression => {
                // `x as T`: the cast type; otherwise the first argument.
                let result = self
                    .el(expr)
                    .owned
                    .iter()
                    .copied()
                    .find(|m| self.kind(*m) == M::ReturnParameterMembership)
                    .and_then(|m| self.owned_member(m));
                if let Some(r) = result {
                    let typing = self.owned_rels(r, |k| k == M::FeatureTyping);
                    if let Some(t) = typing.first() {
                        return self.general(*t);
                    }
                }
                let first = self
                    .el(expr)
                    .owned
                    .iter()
                    .copied()
                    .find(|m| self.kind(*m) == M::ParameterMembership)?;
                let arg = self.owned_member(first)?;
                let fv = self
                    .el(arg)
                    .owned
                    .iter()
                    .copied()
                    .find(|m| self.kind(*m) == M::FeatureValue)?;
                let inner = self.owned_member(fv)?;
                self.expression_target(inner)
            }
            M::IndexExpression => {
                let first = self
                    .el(expr)
                    .owned
                    .iter()
                    .copied()
                    .find(|m| self.kind(*m) == M::ParameterMembership)?;
                let arg = self.owned_member(first)?;
                let fv = self
                    .el(arg)
                    .owned
                    .iter()
                    .copied()
                    .find(|m| self.kind(*m) == M::FeatureValue)?;
                let inner = self.owned_member(fv)?;
                self.expression_target(inner)
            }
            M::InvocationExpression | M::ConstructorExpression => {
                let m = self
                    .el(expr)
                    .owned
                    .iter()
                    .copied()
                    .find(|m| self.kind(*m) == M::Membership)?;
                let f = self.end_target(m, Role::Target)?;
                // The result parameter of a function, else the type itself.
                if self.kind(expr) == M::InvocationExpression {
                    if let Some(result) = self.result_parameter(f) {
                        return Some(result);
                    }
                }
                Some(f)
            }
            _ => Some(expr),
        }
    }

    /// The result parameter of a function/expression (own or inherited).
    pub fn result_parameter(&mut self, f: ElementRef) -> Option<ElementRef> {
        if let Some(m) = self
            .el(f)
            .owned
            .iter()
            .copied()
            .find(|m| self.kind(*m) == M::ReturnParameterMembership)
        {
            return self.owned_member(m);
        }
        // Inherited result.
        let supers = self.supertypes(f);
        for s in supers.iter() {
            if *s != f && self.kind(*s).is_function() || self.kind(*s).is_expression() {
                if let Some(r) = self.owned_result(*s) {
                    return Some(r);
                }
            }
        }
        None
    }

    fn owned_result(&self, f: ElementRef) -> Option<ElementRef> {
        self.el(f)
            .owned
            .iter()
            .copied()
            .find(|m| self.kind(*m) == M::ReturnParameterMembership)
            .and_then(|m| self.owned_member(m))
    }

    /// `featureTarget`: the last chaining feature, or the feature itself.
    pub fn feature_target(&mut self, f: ElementRef) -> ElementRef {
        let chainings = self.owned_rels(f, |k| k == M::FeatureChaining);
        match chainings.last() {
            Some(c) => self.general(*c).unwrap_or(f),
            None => f,
        }
    }

    /// The ConjugatedPortDefinition owned by a PortDefinition.
    pub fn conjugated_port_definition(&self, port_def: ElementRef) -> Option<ElementRef> {
        if self.kind(port_def) == M::ConjugatedPortDefinition {
            return Some(port_def);
        }
        self.owned_members(port_def)
            .into_iter()
            .find(|m| self.kind(*m) == M::ConjugatedPortDefinition)
    }

    // ------------------------------------------------------------------
    // Basic resolution

    /// Resolve a qualified name with `ns` as the local namespace (basic
    /// resolution, 8.2.3.5.1).
    pub fn resolve_qualified(
        &mut self,
        ns: ElementRef,
        segments: &[Box<str>],
    ) -> Option<Resolution> {
        let (first, rest) = segments.split_first()?;
        let mut res = self.resolve_full(ns, first)?;
        for seg in rest {
            let target = self.namespace_for_lookup(res.element);
            res = self.resolve_visible(target, seg)?;
        }
        Some(res)
    }

    fn resolve_qualified_from_global(&mut self, segments: &[Box<str>]) -> Option<Resolution> {
        let (first, rest) = segments.split_first()?;
        let mut res = self.resolve_global(first)?;
        for seg in rest {
            let target = self.namespace_for_lookup(res.element);
            res = self.resolve_visible(target, seg)?;
        }
        Some(res)
    }

    /// Resolve `segments` in a feature: the first segment by local
    /// resolution among the feature's memberships (no outward search),
    /// the rest by visible resolution.
    fn resolve_in_feature(
        &mut self,
        feature: ElementRef,
        segments: &[Box<str>],
        _global: bool,
    ) -> Option<Resolution> {
        let (first, rest) = segments.split_first()?;
        let ns = self.namespace_for_lookup(feature);
        let mut res = self.resolve_local_in(ns, first);
        if res.is_none() && rest.is_empty() {
            return None;
        }
        if res.is_none() {
            // A qualified name in a chain (`a.B::c`): resolve normally from
            // the feature's namespace.
            res = self.resolve_qualified(ns, segments);
            return res;
        }
        let mut res = res?;
        for seg in rest {
            let target = self.namespace_for_lookup(res.element);
            res = self.resolve_visible(target, seg)?;
        }
        Some(res)
    }

    /// For lookups "in" an element: a feature chain is looked up in its
    /// target; an alias's element is itself.
    fn namespace_for_lookup(&mut self, e: ElementRef) -> ElementRef {
        if self.kind(e).is_feature() {
            let chainings = self.owned_rels(e, |k| k == M::FeatureChaining);
            if !chainings.is_empty() {
                return self.feature_target(e);
            }
        }
        e
    }

    /// Full resolution of a simple name relative to `ns` (8.2.3.5.4).
    pub fn resolve_full(&mut self, ns: ElementRef, name: &str) -> Option<Resolution> {
        let mut cur = ns;
        loop {
            if let Some(res) = self.resolve_local_in(cur, name) {
                return Some(res);
            }
            match self.outer_namespace(cur) {
                Some(o) => cur = o,
                None => return self.resolve_global(name),
            }
        }
    }

    /// The namespace enclosing `ns` for full resolution.
    fn outer_namespace(&self, ns: ElementRef) -> Option<ElementRef> {
        let rel = self.owner(ns)?;
        if self.kind(rel).is_membership() {
            return self.owner(rel);
        }
        // An element owned by a non-membership relationship (e.g. the
        // FilterPackage of an import, a chain feature): continue from the
        // relationship's owner.
        let owner = self.owner(rel)?;
        Some(owner)
    }

    /// Local resolution: the first membership of `ns` (owned, imported or
    /// inherited, any visibility) with the given name.
    pub fn resolve_local_in(&mut self, ns: ElementRef, name: &str) -> Option<Resolution> {
        let table = self.table(TableKey::All(ns))?;
        table.lookup(name).map(|e| Resolution {
            membership: e.membership,
            element: e.element,
        })
    }

    /// Resolution of a redefined feature's name in a general type: like
    /// local resolution, but redefined inherited features are not hidden.
    pub fn resolve_in_redefinable(&mut self, ns: ElementRef, name: &str) -> Option<Resolution> {
        if let Some(res) = self.resolve_local_in(ns, name) {
            return Some(res);
        }
        let table = self.table(TableKey::Redefinable(ns))?;
        table.lookup(name).map(|e| Resolution {
            membership: e.membership,
            element: e.element,
        })
    }

    /// Visible resolution: the first public membership of `ns` with the name.
    pub fn resolve_visible(&mut self, ns: ElementRef, name: &str) -> Option<Resolution> {
        let table = self.table(TableKey::Visible(ns, false, false))?;
        table.lookup(name).map(|e| Resolution {
            membership: e.membership,
            element: e.element,
        })
    }

    /// Resolution in the global namespace: visible memberships of every
    /// root namespace (library and project).
    pub fn resolve_global(&mut self, name: &str) -> Option<Resolution> {
        self.build_global();
        if let Some(e) = self.cache.global.get(name).and_then(|v| v.first()) {
            return Some(Resolution {
                membership: e.membership,
                element: e.element,
            });
        }
        None
    }

    fn build_global(&mut self) {
        if self.cache.global_built {
            return;
        }
        self.cache.global_built = true;
        let mut roots: Vec<ElementRef> = Vec::new();
        if let Some(lib) = self.lib {
            roots.extend(lib.graph.files.iter().map(|f| f.root));
        }
        roots.extend(self.g.get().files.iter().map(|f| f.root));
        let mut global: HashMap<Box<str>, Vec<Entry>> = HashMap::new();
        for root in roots {
            // Root namespaces' visible memberships: their public owned
            // members and public imports.
            let Some(table) = self.table(TableKey::Visible(root, false, false)) else {
                continue;
            };
            for entry in &table.entries {
                for name in self.entry_names(entry) {
                    global.entry(name).or_default().push(*entry);
                }
            }
        }
        self.cache.global = global;
    }

    /// Resolve a library element by qualified name (cached).
    pub fn library_element(&mut self, qn: &str) -> Option<ElementRef> {
        if let Some(l) = self.lib {
            if let Some(v) = l.lib_cache.get(qn) {
                return *v;
            }
        }
        if let Some(v) = self.cache.lib_cache.get(qn) {
            return *v;
        }
        let segments: Vec<Box<str>> = split_qualified_name(qn);
        let cuts = self.cuts;
        let res = self
            .resolve_qualified_from_global(&segments)
            .map(|r| r.element);
        if res.is_some() || self.cuts == cuts {
            self.cache.lib_cache.insert(qn.to_owned(), res);
        }
        res
    }

    // ------------------------------------------------------------------
    // Names

    /// Effective name (`name`, KerML 8.3.2.1 with the `effectiveName()`
    /// redefinitions of Feature and the SysML overrides).
    pub fn name(&mut self, e: ElementRef) -> Option<Box<str>> {
        {
            let el = self.el(e);
            if el.name.is_some() || el.short_name.is_some() {
                return el.name.clone();
            }
        }
        if self.is_lib(e) {
            let l = self.lib.unwrap();
            return match l.state.names.get((e.0 - l.graph.base) as usize) {
                Some(Slot::Done(n)) => n.clone(),
                _ => None,
            };
        }
        let i = self.local_index(e);
        if let Some(Slot::Done(n)) = self.st.get().names.get(i) {
            return n.clone();
        }
        if !self.cache.names_busy.insert(e) {
            self.cuts += 1;
            return None;
        }
        let cuts = self.cuts;
        let result = self.compute_effective_name(e);
        self.cache.names_busy.remove(&e);
        if self.cuts == cuts || result.is_some() {
            if let Some(st) = self.st.get_mut() {
                ensure(&mut st.names, i);
                st.names[i] = Slot::Done(result.clone());
            }
        }
        result
    }

    fn compute_effective_name(&mut self, e: ElementRef) -> Option<Box<str>> {
        if self.kind(e) == M::ConjugatedPortDefinition {
            let pc = self.owned_rels(e, |k| k == M::PortConjugation);
            let original = self.general(*pc.first()?)?;
            let n = self.name(original)?;
            return Some(format!("~{n}").into());
        }
        let f = self.naming_feature(e)?;
        self.name(f)
    }

    /// Effective short name.
    pub fn short_name(&mut self, e: ElementRef) -> Option<Box<str>> {
        {
            let el = self.el(e);
            if el.name.is_some() || el.short_name.is_some() {
                return el.short_name.clone();
            }
        }
        if self.kind(e) == M::ConjugatedPortDefinition {
            return None;
        }
        if self.is_lib(e) {
            let l = self.lib.unwrap();
            return match l.state.short_names.get((e.0 - l.graph.base) as usize) {
                Some(Slot::Done(n)) => n.clone(),
                _ => None,
            };
        }
        let i = self.local_index(e);
        if let Some(Slot::Done(n)) = self.st.get().short_names.get(i) {
            return n.clone();
        }
        if !self.cache.short_names_busy.insert(e) {
            self.cuts += 1;
            return None;
        }
        let cuts = self.cuts;
        let result = match self.naming_feature(e) {
            Some(f) => self.short_name(f),
            None => None,
        };
        self.cache.short_names_busy.remove(&e);
        if self.cuts == cuts || result.is_some() {
            if let Some(st) = self.st.get_mut() {
                ensure(&mut st.short_names, i);
                st.short_names[i] = Slot::Done(result.clone());
            }
        }
        result
    }

    /// `namingFeature()` (KerML 8.3.3.3.4) with the SysML overrides.
    pub fn naming_feature(&mut self, e: ElementRef) -> Option<ElementRef> {
        let kind = self.kind(e);
        if !kind.is_feature() {
            return None;
        }
        let owning = self.owner(e).map(|r| self.kind(r));
        // Usage: a variant is named by its referenced feature.
        if owning == Some(M::VariantMembership) {
            return self.referenced_feature(e);
        }
        if matches!(
            kind,
            M::PerformActionUsage | M::ExhibitStateUsage | M::IncludeUseCaseUsage
        ) {
            if let Some(f) = self.referenced_feature(e) {
                return Some(f);
            }
        }
        if owning == Some(M::RequirementConstraintMembership) {
            if let Some(f) = self.referenced_feature(e) {
                return Some(self.feature_target(f));
            }
        }
        // The first owned redefinition (explicit first, then implied).
        self.ensure_implied_redefinitions(e);
        let redefs = self.owned_rels(e, |k| k == M::Redefinition);
        let first = *redefs.first()?;
        self.general(first)
    }

    /// `ownedReferenceSubsetting.referencedFeature`.
    pub fn referenced_feature(&mut self, e: ElementRef) -> Option<ElementRef> {
        let rs = self.owned_rels(e, |k| k == M::ReferenceSubsetting);
        self.general(*rs.first()?)
    }

    /// Names (name and short name) under which a membership entry is known.
    pub fn entry_names(&mut self, entry: &Entry) -> Vec<Box<str>> {
        self.membership_names(entry.membership, entry.element)
    }

    fn membership_names(&mut self, membership: ElementRef, element: ElementRef) -> Vec<Box<str>> {
        let mut out = Vec::new();
        let mk = self.kind(membership);
        if mk.is_owning_membership() {
            if let Some(n) = self.name(element) {
                out.push(n);
            }
            if let Some(s) = self.short_name(element) {
                out.push(s);
            }
        } else {
            let el = self.el(membership);
            if let Some(n) = &el.name {
                out.push(n.clone());
            }
            if let Some(s) = &el.short_name {
                out.push(s.clone());
            }
        }
        out
    }

    // ------------------------------------------------------------------
    // Tables

    fn frozen_table(&self, key: TableKey) -> Option<Arc<Table>> {
        let l = self.lib?;
        l.tables.get(&key).cloned()
    }

    /// The membership table for `key`, computed on demand.
    pub fn table(&mut self, key: TableKey) -> Option<Arc<Table>> {
        if let Some(t) = self.frozen_table(key) {
            return Some(t);
        }
        if let Some(t) = self.cache.tables.get(&key) {
            return Some(t.clone());
        }
        if !self.cache.tables_in_progress.insert(key) {
            self.cuts += 1;
            return None;
        }
        let cuts = self.cuts;
        let entries = self.compute_table(key);
        let mut table = Table {
            entries,
            index: HashMap::new(),
        };
        for (i, entry) in table.entries.clone().iter().enumerate() {
            for name in self.entry_names(entry) {
                table.index.entry(name).or_default().push(i as u32);
            }
        }
        let table = Arc::new(table);
        self.cache.tables_in_progress.remove(&key);
        if self.cuts == cuts {
            self.cache.tables.insert(key, table.clone());
        }
        Some(table)
    }

    fn compute_table(&mut self, key: TableKey) -> Vec<Entry> {
        match key {
            TableKey::All(ns) => {
                let mut out = self.owned_entries(ns, None);
                out.extend(self.imported_entries(ns, None));
                if self.kind(ns).is_type() {
                    if let Some(t) = self.table(TableKey::Inherited(ns, false)) {
                        out.extend(t.entries.iter().copied());
                    }
                }
                out
            }
            TableKey::Visible(ns, recursive, include_all) => {
                let vis = if include_all {
                    None
                } else {
                    Some(Visibility::Public)
                };
                let mut out = self.owned_entries(ns, vis);
                out.extend(self.imported_entries(ns, vis));
                if self.kind(ns).is_type() {
                    if let Some(t) = self.table(TableKey::Inherited(ns, recursive)) {
                        out.extend(
                            t.entries
                                .iter()
                                .filter(|e| include_all || e.visibility == Visibility::Public)
                                .copied(),
                        );
                    }
                }
                if recursive {
                    let members: Vec<(ElementRef, ElementRef)> = self
                        .el(ns)
                        .owned
                        .iter()
                        .filter_map(|m| self.owned_member(*m).map(|e| (*m, e)))
                        .collect();
                    for (m, member) in members {
                        if !self.kind(member).is_namespace() {
                            continue;
                        }
                        if !include_all && self.el(m).visibility != Visibility::Public {
                            continue;
                        }
                        if let Some(t) = self.table(TableKey::Visible(member, true, include_all)) {
                            out.extend(t.entries.iter().copied());
                        }
                    }
                }
                out
            }
            TableKey::NonPrivate(t, exclude_implied) => {
                let mut out = self.owned_entries(t, Some(Visibility::Public));
                out.extend(self.owned_entries(t, Some(Visibility::Protected)));
                out.extend(self.imported_entries(t, Some(Visibility::Public)));
                out.extend(self.imported_entries(t, Some(Visibility::Protected)));
                if self.kind(t).is_type() {
                    if let Some(inh) = self.table(TableKey::Inherited(t, exclude_implied)) {
                        out.extend(inh.entries.iter().copied());
                    }
                }
                out
            }
            TableKey::Inherited(t, exclude_implied) => self.inherited_entries(t, exclude_implied),
            TableKey::Redefinable(t) => {
                let mut out = self.owned_entries(t, None);
                out.extend(self.imported_entries(t, None));
                if self.kind(t).is_type() {
                    let supers = self.supertypes(t);
                    for s in supers.iter() {
                        if *s == t {
                            continue;
                        }
                        if let Some(table) = self.table(TableKey::Redefinable(*s)) {
                            out.extend(
                                table
                                    .entries
                                    .iter()
                                    .filter(|e| {
                                        e.visibility != Visibility::Private
                                            || e.origin == Origin::Inherited
                                    })
                                    .map(|e| Entry {
                                        origin: Origin::Inherited,
                                        ..*e
                                    }),
                            );
                        }
                    }
                }
                out
            }
        }
    }

    /// Owned memberships of `ns` (with the given visibility, if any).
    fn owned_entries(&mut self, ns: ElementRef, vis: Option<Visibility>) -> Vec<Entry> {
        let owned = self.el(ns).owned.clone();
        let mut out = Vec::new();
        for m in owned {
            let mk = self.kind(m);
            if !mk.is_membership() {
                continue;
            }
            let v = self.el(m).visibility;
            if vis.is_some_and(|want| want != v) {
                continue;
            }
            let element = if mk.is_owning_membership() {
                match self.owned_member(m) {
                    Some(e) => e,
                    None => continue,
                }
            } else {
                match self.end_target(m, Role::Target) {
                    Some(e) => e,
                    None => continue,
                }
            };
            out.push(Entry {
                membership: m,
                element,
                visibility: v,
                origin: Origin::Owned,
            });
        }
        out
    }

    /// Memberships imported into `ns` by imports with the given visibility.
    fn imported_entries(&mut self, ns: ElementRef, vis: Option<Visibility>) -> Vec<Entry> {
        let imports: Vec<ElementRef> = self
            .el(ns)
            .owned
            .iter()
            .copied()
            .filter(|r| self.kind(*r).is_import())
            .collect();
        let mut out = Vec::new();
        for imp in imports {
            let v = self.el(imp).visibility;
            if vis.is_some_and(|want| want != v) {
                continue;
            }
            let flags = self.el(imp).flags;
            let recursive = flags.contains(Flags::RECURSIVE);
            let all = flags.contains(Flags::IMPORT_ALL);
            let kind = self.kind(imp);
            let target = self.el(imp).end(Role::Target);
            let mut push = |out: &mut Vec<Entry>, e: &Entry| {
                out.push(Entry {
                    membership: e.membership,
                    element: e.element,
                    visibility: v,
                    origin: Origin::Imported,
                })
            };
            if kind.is_namespace_import() {
                let imported_ns = match target {
                    Some(Target::Owned(p)) => Some(p),
                    Some(t) => self.target_element(t),
                    None => None,
                };
                let Some(imported_ns) = imported_ns else {
                    continue;
                };
                if imported_ns == ns {
                    continue;
                }
                if self.kind(imported_ns) == M::Package && matches!(target, Some(Target::Owned(_)))
                {
                    // FilterPackage: its imported memberships, filtered.
                    let entries = self.filter_package_entries(imported_ns);
                    for e in &entries {
                        push(&mut out, e);
                    }
                    continue;
                }
                if let Some(t) = self.table(TableKey::Visible(imported_ns, recursive, all)) {
                    for e in t.entries.iter() {
                        push(&mut out, e);
                    }
                }
            } else {
                let Some(Target::Name(r)) = target else {
                    continue;
                };
                let Some(res) = self.resolve_ref(r) else {
                    continue;
                };
                push(
                    &mut out,
                    &Entry {
                        membership: res.membership,
                        element: res.element,
                        visibility: v,
                        origin: Origin::Imported,
                    },
                );
                if recursive && self.kind(res.element).is_namespace() && res.element != ns {
                    if let Some(t) = self.table(TableKey::Visible(res.element, true, all)) {
                        for e in t.entries.iter() {
                            push(&mut out, e);
                        }
                    }
                }
            }
        }
        out
    }

    /// The memberships a FilterPackage makes available: those imported by
    /// its import whose elements pass the filter conditions. Conditions
    /// are not evaluated in this stage (metadata evaluation is not
    /// implemented); every imported membership passes.
    fn filter_package_entries(&mut self, package: ElementRef) -> Vec<Entry> {
        self.imported_entries(package, None)
    }

    /// Inherited memberships of a type (KerML 8.3.3.1.10
    /// `inheritedMemberships`, with `removeRedefinedFeatures`).
    fn inherited_entries(&mut self, t: ElementRef, exclude_implied: bool) -> Vec<Entry> {
        let supers = if exclude_implied {
            self.supertypes_explicit(t)
        } else {
            self.supertypes(t)
        };
        let mut all: Vec<Entry> = Vec::new();
        let mut seen: HashSet<ElementRef> = HashSet::new();
        for s in supers.iter() {
            if *s == t {
                continue;
            }
            let Some(table) = self.table(TableKey::NonPrivate(*s, exclude_implied)) else {
                continue;
            };
            for e in table.entries.iter() {
                if seen.insert(e.membership) {
                    all.push(Entry {
                        origin: Origin::Inherited,
                        ..*e
                    });
                }
            }
        }
        // removeRedefinedFeatures.
        let mut strictly_redefined: HashSet<ElementRef> = HashSet::new();
        for e in &all {
            if self.kind(e.element).is_feature() {
                for r in self.all_redefined(e.element).iter() {
                    if *r != e.element {
                        strictly_redefined.insert(*r);
                    }
                }
            }
        }
        let mut owned_redefined: HashSet<ElementRef> = HashSet::new();
        let owned_features: Vec<ElementRef> = self
            .el(t)
            .owned
            .iter()
            .copied()
            .filter(|m| self.kind(*m).is_feature_membership())
            .filter_map(|m| self.owned_member(m))
            .collect();
        for f in owned_features {
            self.ensure_implied_redefinitions(f);
            for r in self.owned_rels(f, |k| k == M::Redefinition) {
                if let Some(g) = self.general(r) {
                    owned_redefined.insert(g);
                }
            }
        }
        let mut out = Vec::with_capacity(all.len());
        for e in all {
            if strictly_redefined.contains(&e.element) {
                continue;
            }
            if !owned_redefined.is_empty() && self.kind(e.element).is_feature() {
                let redefs = self.all_redefined(e.element);
                if redefs.iter().any(|r| owned_redefined.contains(r)) {
                    continue;
                }
            }
            out.push(e);
        }
        out
    }

    /// The feature itself and every feature it directly or indirectly
    /// redefines.
    pub fn all_redefined(&mut self, f: ElementRef) -> Arc<[ElementRef]> {
        if let Some(v) = self.cache.redefined.get(&f) {
            return v.clone();
        }
        let mut out = vec![f];
        let mut stack = vec![f];
        let cuts = self.cuts;
        while let Some(x) = stack.pop() {
            self.ensure_implied_redefinitions(x);
            for r in self.owned_rels(x, |k| k == M::Redefinition) {
                if let Some(g) = self.general(r) {
                    if !out.contains(&g) {
                        out.push(g);
                        stack.push(g);
                    }
                }
            }
        }
        let v: Arc<[ElementRef]> = out.into();
        if self.cuts == cuts {
            self.cache.redefined.insert(f, v.clone());
        }
        v
    }

    // ------------------------------------------------------------------
    // Supertypes

    /// The general types of `t`'s owned specializations, explicit then
    /// implied (`supertypes(false)`); for a conjugated type, the original
    /// type only.
    pub fn supertypes(&mut self, t: ElementRef) -> Arc<[ElementRef]> {
        self.supertypes_impl(t, false)
    }

    /// `supertypes(true)`: excluding implied specializations.
    pub fn supertypes_explicit(&mut self, t: ElementRef) -> Arc<[ElementRef]> {
        self.supertypes_impl(t, true)
    }

    fn supertypes_impl(&mut self, t: ElementRef, explicit_only: bool) -> Arc<[ElementRef]> {
        if self.is_lib(t) {
            let l = self.lib.unwrap();
            let i = (t.0 - l.graph.base) as usize;
            let slots = if explicit_only {
                &l.state.supers_explicit
            } else {
                &l.state.supers
            };
            if let Some(Slot::Done(v)) = slots.get(i) {
                return v.clone();
            }
            // Not precomputed (non-type elements): compute without caching.
            return self.compute_supertypes(t, explicit_only).into();
        }
        let i = self.local_index(t);
        {
            let st = self.st.get();
            let slots = if explicit_only {
                &st.supers_explicit
            } else {
                &st.supers
            };
            if let Some(Slot::Done(v)) = slots.get(i) {
                return v.clone();
            }
        }
        if !self.cache.supers_busy.insert((t, explicit_only)) {
            self.cuts += 1;
            return Arc::from(Vec::new());
        }
        let cuts = self.cuts;
        let result: Arc<[ElementRef]> = self.compute_supertypes(t, explicit_only).into();
        self.cache.supers_busy.remove(&(t, explicit_only));
        if self.cuts == cuts {
            if let Some(st) = self.st.get_mut() {
                let slots = if explicit_only {
                    &mut st.supers_explicit
                } else {
                    &mut st.supers
                };
                ensure(slots, i);
                slots[i] = Slot::Done(result.clone());
            }
        }
        result
    }

    fn compute_supertypes(&mut self, t: ElementRef, explicit_only: bool) -> Vec<ElementRef> {
        let kind = self.kind(t);
        if kind == M::ConjugatedPortDefinition {
            let pc = self.owned_rels(t, |k| k == M::PortConjugation);
            return pc
                .first()
                .and_then(|r| self.general(*r))
                .into_iter()
                .collect();
        }
        let conjugations = self.owned_rels(t, |k| k == M::Conjugation);
        if let Some(c) = conjugations.first() {
            return self.general(*c).into_iter().collect();
        }
        if !explicit_only {
            self.ensure_implied(t);
        }
        let rels: Vec<ElementRef> = self.el(t).owned.clone();
        let mut out = Vec::new();
        for r in rels {
            let rk = self.kind(r);
            if !rk.is_specialization() {
                continue;
            }
            if explicit_only && self.el(r).is_implied() {
                continue;
            }
            // Standalone specializations owned by a namespace do not make
            // the namespace a subtype.
            if self.el(r).end(Role::Source).is_some() {
                continue;
            }
            if let Some(g) = self.general(r) {
                let g = if rk == M::CrossSubsetting {
                    continue;
                } else {
                    g
                };
                out.push(g);
            }
        }
        // A feature chain's members are those of its last feature.
        if kind.is_feature() {
            let chainings = self.owned_rels(t, |k| k == M::FeatureChaining);
            if let Some(last) = chainings.last() {
                if let Some(f) = self.general(*last) {
                    if !out.contains(&f) {
                        out.push(f);
                    }
                }
            }
        }
        // Invocation and constructor expressions are typed by their
        // instantiated type (implied FeatureTyping).
        if is_instantiation(kind)
            && !matches!(
                kind,
                M::OperatorExpression
                    | M::FeatureChainExpression
                    | M::IndexExpression
                    | M::SelectExpression
                    | M::CollectExpression
            )
        {
            if let Some(m) = self
                .el(t)
                .owned
                .iter()
                .copied()
                .find(|m| self.kind(*m) == M::Membership)
            {
                if let Some(f) = self.end_target(m, Role::Target) {
                    out.push(f);
                }
            }
        }
        out
    }

    /// All direct and indirect supertypes of `t` (excluding `t`).
    pub fn all_supertypes(&mut self, t: ElementRef) -> Arc<HashSet<ElementRef>> {
        if let Some(l) = self.lib {
            if let Some(v) = l.all_supertypes.get(&t) {
                return v.clone();
            }
        }
        if let Some(v) = self.cache.all_supertypes.get(&t) {
            return v.clone();
        }
        let cuts = self.cuts;
        let mut out: HashSet<ElementRef> = HashSet::new();
        let mut stack: Vec<ElementRef> = self.supertypes(t).to_vec();
        while let Some(s) = stack.pop() {
            if s == t || !out.insert(s) {
                continue;
            }
            if let Some(l) = self.lib {
                if let Some(v) = l.all_supertypes.get(&s) {
                    out.extend(v.iter().copied());
                    continue;
                }
            }
            if let Some(v) = self.cache.all_supertypes.get(&s) {
                let v = v.clone();
                out.extend(v.iter().copied());
                continue;
            }
            stack.extend(self.supertypes(s).iter().copied());
        }
        let out = Arc::new(out);
        if self.cuts == cuts {
            self.cache.all_supertypes.insert(t, out.clone());
        }
        out
    }

    /// Whether `t` directly or indirectly specializes `general`.
    pub fn specializes(&mut self, t: ElementRef, general: ElementRef) -> bool {
        t == general || self.all_supertypes(t).contains(&general)
    }

    // ------------------------------------------------------------------
    // Implied relationships (see `crate::implied`)

    pub fn implied_state(&self, e: ElementRef) -> u8 {
        if self.is_lib(e) {
            return 4;
        }
        match &self.st {
            // A built model never adds implied relationships.
            Access::Read(_) => 4,
            Access::Write(st) => st.implied.get(self.local_index(e)).copied().unwrap_or(0),
        }
    }

    pub fn set_implied_state(&mut self, e: ElementRef, s: u8) {
        let i = self.local_index(e);
        if let Some(st) = self.st.get_mut() {
            ensure(&mut st.implied, i);
            st.implied[i] = s;
        }
    }

    /// Add an implied relationship of `kind` from `source` to `target`.
    pub fn add_implied(
        &mut self,
        source: ElementRef,
        kind: Metaclass,
        target: ElementRef,
    ) -> ElementRef {
        let span = self.el(source).span;
        let mut el = Element::new(kind, span);
        el.flags.insert(Flags::IMPLIED);
        el.ends.push(End {
            role: Role::Target,
            target: Target::Element(target),
        });
        let g = self
            .g
            .get_mut()
            .expect("implied relationships are only added while building");
        let r = g.push(el);
        g.attach(source, r);
        // Derived caches of the source may now be stale.
        let i = self.local_index(source);
        if let Some(st) = self.st.get_mut() {
            if let Some(s) = st.supers.get_mut(i) {
                *s = Slot::Pending;
            }
        }
        self.cache.all_supertypes.remove(&source);
        self.cache.redefined.remove(&source);
        r
    }
}

/// Whether `k` is an `InstantiationExpression` (invocation, operator,
/// constructor and their kin).
pub(crate) fn is_instantiation(k: Metaclass) -> bool {
    matches!(
        k,
        M::InvocationExpression
            | M::OperatorExpression
            | M::FeatureChainExpression
            | M::CollectExpression
            | M::SelectExpression
            | M::IndexExpression
            | M::ConstructorExpression
            | M::TriggerInvocationExpression
    )
}

/// Split a qualified name (`A::'b c'::d`) into unescaped segments.
pub(crate) fn split_qualified_name(qn: &str) -> Vec<Box<str>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut chars = qn.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            '\\' if in_quote => {
                cur.push(c);
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            ':' if !in_quote && chars.peek() == Some(&':') => {
                chars.next();
                out.push(crate::name::unescape_name(&cur).into());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    out.push(crate::name::unescape_name(&cur).into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_qualified_names() {
        assert_eq!(
            split_qualified_name("A::'b::c'::d"),
            vec!["A".into(), "b::c".into(), "d".into()] as Vec<Box<str>>
        );
    }
}
