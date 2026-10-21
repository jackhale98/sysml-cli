//! Lowering of namespaces, declarations and relationships.

use tree_sitter::Node;

use super::{all_kids, kids, Lowerer, Prefix};
use crate::graph::{
    Direction, ElementRef, End, Flags, PortionKind, Role, Target, Value, Visibility,
};
use crate::metaclass::Metaclass as M;
use crate::metaclass::Metaclass;

/// How the members of a body are owned.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BodyKind {
    /// Package / namespace / root: every member via `OwningMembership`.
    Package,
    /// Type body: features via `FeatureMembership`.
    Type,
    /// Metadata body: features are `MetadataBodyUsage`s (redefinitions).
    Metadata,
}

fn visibility_of(text: &str) -> Option<Visibility> {
    match text {
        "public" => Some(Visibility::Public),
        "protected" => Some(Visibility::Protected),
        "private" => Some(Visibility::Private),
        _ => None,
    }
}

/// The single anonymous token of a `modifier` / `visibility` node.
fn token_of(node: Node) -> &'static str {
    let k = node.child(0).map(|c| c.kind()).unwrap_or("");
    // Return a static string for the known modifiers.
    const KNOWN: &[&str] = &[
        "abstract",
        "variation",
        "variant",
        "individual",
        "readonly",
        "derived",
        "nonunique",
        "ordered",
        "in",
        "out",
        "inout",
        "composite",
        "conjugate",
        "const",
        "constant",
        "disjoint",
        "portion",
        "var",
        "ref",
        "public",
        "private",
        "protected",
    ];
    KNOWN.iter().find(|m| **m == k).copied().unwrap_or("")
}

const BODIES: &[&str] = &[
    "definition_body",
    "state_body",
    "requirement_body",
    "constraint_body",
    "enumeration_body",
    "metadata_body",
    "package_body",
];

impl<'g, 's> Lowerer<'g, 's> {
    // ------------------------------------------------------------------
    // Bodies

    /// Lower the elements of a body node into `ns`.
    pub(crate) fn body(&mut self, ns: ElementRef, body: Node) {
        let kind = self.body_kind(ns, body);
        self.body_as(ns, body, kind);
    }

    pub(crate) fn body_kind(&self, ns: ElementRef, body: Node) -> BodyKind {
        if body.kind() == "metadata_body" {
            BodyKind::Metadata
        } else if self.kind_of(ns).is_type() {
            BodyKind::Type
        } else {
            BodyKind::Package
        }
    }

    pub(crate) fn body_as(&mut self, ns: ElementRef, body: Node, kind: BodyKind) {
        let mut prefix = Prefix::default();
        for k in all_kids(body) {
            match k.kind() {
                "{" | "}" | ";" | "line_comment" => {}
                "visibility" => {
                    prefix.start.get_or_insert(k.start_byte());
                    prefix.visibility = visibility_of(self.text(k));
                }
                "modifier" => {
                    prefix.start.get_or_insert(k.start_byte());
                    prefix.modifiers.push(token_of(k));
                }
                "hash_tag" => {
                    prefix.start.get_or_insert(k.start_byte());
                    prefix.metadata.push(k);
                }
                "block_comment" => {
                    // A REGULAR_COMMENT in body position is a Comment
                    // element; `//*` notes are not.
                    if self.text(k).starts_with("/*") && prefix.is_empty() {
                        self.comment(ns, k, None);
                    }
                }
                "ERROR" => {
                    prefix = Prefix::default();
                }
                _ if k.is_named() => {
                    let p = std::mem::take(&mut prefix);
                    self.member(ns, k, p, kind);
                }
                _ => {}
            }
        }
        // Dangling prefix metadata (e.g. at the end of a body).
        for m in std::mem::take(&mut prefix.metadata) {
            let mu = self.prefix_metadata_feature(m);
            self.own_member(ns, M::OwningMembership, mu, None);
        }
    }

    /// The default membership for an owned member of a namespace.
    fn default_membership(&self, kind: BodyKind, member: Metaclass) -> Metaclass {
        match kind {
            BodyKind::Package => M::OwningMembership,
            BodyKind::Type | BodyKind::Metadata => {
                if member.is_feature()
                    && !member.is_metadata_feature()
                    && !matches!(member, M::Multiplicity | M::MultiplicityRange)
                {
                    M::FeatureMembership
                } else {
                    M::OwningMembership
                }
            }
        }
    }

    /// Lower one body element (also used for expression bodies).
    pub(crate) fn member_pub<'t>(
        &mut self,
        ns: ElementRef,
        node: Node<'t>,
        prefix: Prefix<'t>,
        kind: BodyKind,
    ) {
        self.member(ns, node, prefix, kind)
    }

    /// Lower one body element.
    fn member<'t>(
        &mut self,
        ns: ElementRef,
        node: Node<'t>,
        mut prefix: Prefix<'t>,
        kind: BodyKind,
    ) {
        if super::expr::is_expression_kind(node.kind()) {
            // A bare expression in a constraint or calculation body: the
            // result expression.
            let x = self.expression(node);
            self.own_member(ns, M::ResultExpressionMembership, x, prefix.visibility);
            return;
        }
        let has_then = || kids(node).iter().any(|k| k.kind() == "then");
        match node.kind() {
            "if_action" if has_then() => self.guarded_succession(ns, node, &prefix),
            "accept_action" if has_then() => {
                let t = self.transition(node, &prefix);
                self.own_member(ns, M::FeatureMembership, t, prefix.visibility);
            }
            "package_declaration" | "namespace_declaration" => {
                self.collect_inner_prefix(node, &mut prefix);
                let p = self.package(node, &prefix);
                self.own_member(ns, M::OwningMembership, p, prefix.visibility);
            }
            "import_statement" | "expose_statement" => self.import(ns, node, &prefix),
            "alias_declaration" => self.alias(ns, node, &prefix),
            "comment_element" => self.comment(ns, node, prefix.visibility),
            "doc_comment" => {
                let d = self.documentation(node);
                self.own_member(ns, M::OwningMembership, d, prefix.visibility);
            }
            "textual_representation" => {
                let r = self.textual_representation(node);
                self.own_member(ns, M::OwningMembership, r, prefix.visibility);
            }
            "result_expression" => {
                if let Some(e) = node.named_child(0) {
                    let x = self.expression(e);
                    self.own_member(ns, M::ResultExpressionMembership, x, prefix.visibility);
                }
            }
            "expression_statement" => {
                // `expr;` in a body: in a calculation/function this is the
                // result expression; otherwise an owned expression feature.
                if let Some(e) = node.named_child(0) {
                    let x = self.expression(e);
                    self.own_member(ns, M::ResultExpressionMembership, x, prefix.visibility);
                }
            }
            "filter_statement" => {
                for f in kids(node) {
                    if f.kind() == "filter_expression" {
                        let x = self.filter_expression(f);
                        self.own_member(ns, M::ElementFilterMembership, x, prefix.visibility);
                    }
                }
            }
            "metadata_annotation" | "metadata_usage" => {
                let mu = self.metadata_usage(node, &prefix);
                self.own_member(ns, M::OwningMembership, mu, prefix.visibility);
            }
            "dependency_statement" => {
                let d = self.dependency(node, &prefix);
                self.own_member(ns, M::OwningMembership, d, prefix.visibility);
            }
            "specialization_statement"
            | "conjugation_statement"
            | "featuring_statement"
            | "disjoining_statement"
            | "inverse_statement" => {
                let r = self.standalone_relationship(node);
                self.own_member(ns, M::OwningMembership, r, prefix.visibility);
            }
            "multiplicity_definition" => {
                let m = self.multiplicity_member_definition(node, &prefix);
                self.own_member(ns, M::OwningMembership, m, prefix.visibility);
            }
            "enum_member" => {
                let u = self.declaration(node, M::EnumerationUsage, &prefix, kind);
                self.own_member(ns, M::VariantMembership, u, prefix.visibility);
            }
            "metadata_body_usage" => {
                let u = self.metadata_body_usage(node, &prefix);
                self.own_member(ns, M::FeatureMembership, u, prefix.visibility);
            }
            "assignment_statement" => {
                // `name = value;` (metadata bodies; elsewhere treated alike):
                // a feature redefining `name` with a value.
                let u = self.metadata_body_usage(node, &prefix);
                self.own_member(ns, M::FeatureMembership, u, prefix.visibility);
            }
            "subject_declaration" => {
                let u = self.declaration(node, self.ref_usage_kind(), &prefix, kind);
                self.own_member(ns, M::SubjectMembership, u, prefix.visibility);
            }
            "actor_declaration" => {
                let u = self.declaration(node, self.part_kind(), &prefix, kind);
                self.own_member(ns, M::ActorMembership, u, prefix.visibility);
            }
            "stakeholder_declaration" => {
                let u = self.declaration(node, self.part_kind(), &prefix, kind);
                self.own_member(ns, M::StakeholderMembership, u, prefix.visibility);
            }
            "objective_declaration" => {
                let u = self.declaration(node, M::RequirementUsage, &prefix, kind);
                self.own_member(ns, M::ObjectiveMembership, u, prefix.visibility);
            }
            "return_statement" => {
                let k = self.return_kind(node);
                let u = self.declaration(node, k, &prefix, kind);
                self.own_member(ns, M::ReturnParameterMembership, u, prefix.visibility);
            }
            "require_statement" => self.requirement_constraint(ns, node, &prefix, "requirement"),
            "frame_statement" => {
                let u = self.reference_or_declaration(node, M::ConcernUsage, &prefix);
                self.own_member(ns, M::FramedConcernMembership, u, prefix.visibility);
            }
            "verify_statement" => {
                let u = self.reference_or_declaration(node, M::RequirementUsage, &prefix);
                let m = self.own_member(
                    ns,
                    M::RequirementVerificationMembership,
                    u,
                    prefix.visibility,
                );
                self.el(m).value = Value::Kind("requirement");
            }
            "render_statement" => {
                let u = self.reference_or_declaration(node, M::RenderingUsage, &prefix);
                self.own_member(ns, M::ViewRenderingMembership, u, prefix.visibility);
            }
            "constraint_usage" => {
                // `require constraint` / `assume constraint` in requirement bodies.
                let lead = node.child(0).map(|c| c.kind()).unwrap_or("");
                match lead {
                    "require" => self.requirement_constraint(ns, node, &prefix, "requirement"),
                    "assume" => self.requirement_constraint(ns, node, &prefix, "assumption"),
                    _ => self.generic_member(ns, node, prefix, kind),
                }
            }
            "entry_action" | "do_action" | "exit_action" => self.state_subaction(ns, node, &prefix),
            "then_succession" => self.then_succession(ns, node, prefix, kind),
            "member_feature" => {
                let u = self.declaration(node, M::Feature, &prefix, kind);
                self.own_member(ns, M::OwningMembership, u, prefix.visibility);
            }
            "succession_statement" if node.child(0).map(|c| c.kind()) == Some("first") => {
                self.first_statement(ns, node, &prefix);
            }
            "inline_transition" | "else_action" => {
                let t = self.transition(node, &prefix);
                self.own_member(ns, M::FeatureMembership, t, prefix.visibility);
            }
            _ => self.generic_member(ns, node, prefix, kind),
        }
    }

    fn generic_member<'t>(
        &mut self,
        ns: ElementRef,
        node: Node<'t>,
        mut prefix: Prefix<'t>,
        kind: BodyKind,
    ) {
        self.collect_inner_prefix(node, &mut prefix);
        let Some(meta) = self.metaclass_of(node, &prefix) else {
            return;
        };
        let e = self.declaration(node, meta, &prefix, kind);
        let membership = if prefix.has("variant") {
            M::VariantMembership
        } else {
            self.default_membership(kind, self.kind_of(e))
        };
        self.own_member(ns, membership, e, prefix.visibility);
    }

    /// Modifiers / visibility / metadata written inside the node itself.
    fn collect_inner_prefix<'t>(&self, node: Node<'t>, prefix: &mut Prefix<'t>) {
        for k in kids(node) {
            match k.kind() {
                "visibility" => prefix.visibility = visibility_of(self.text(k)),
                "modifier" => prefix.modifiers.push(token_of(k)),
                _ => {}
            }
        }
    }

    pub(crate) fn ref_usage_kind(&self) -> Metaclass {
        if self.kerml {
            M::Feature
        } else {
            M::ReferenceUsage
        }
    }

    fn part_kind(&self) -> Metaclass {
        if self.kerml {
            M::Feature
        } else {
            M::PartUsage
        }
    }

    fn return_kind(&self, node: Node) -> Metaclass {
        for k in kids(node) {
            match k.kind() {
                "attribute" => return M::AttributeUsage,
                "part" => return M::PartUsage,
                "port" => return M::PortUsage,
                "ref" => return self.ref_usage_kind(),
                "feature" => return M::Feature,
                _ => {}
            }
        }
        self.ref_usage_kind()
    }

    // ------------------------------------------------------------------
    // Metaclass selection

    /// The metaclass a declaration node produces.
    pub(crate) fn metaclass_of(&self, node: Node, prefix: &Prefix) -> Option<Metaclass> {
        let tokens: Vec<&str> = kids(node)
            .iter()
            .filter(|k| !k.is_named())
            .map(|k| k.kind())
            .collect();
        let has = |t: &str| tokens.contains(&t);
        let first = tokens.first().copied().unwrap_or("");
        let kerml = self.kerml;
        Some(match node.kind() {
            "definition" => {
                let kw: Vec<&str> = tokens
                    .iter()
                    .take_while(|t| **t != "def")
                    .copied()
                    .collect();
                definition_metaclass(&kw)
            }
            "generic_definition" => M::Definition,
            "state_definition" => M::StateDefinition,
            "enumeration_definition" => M::EnumerationDefinition,
            "usage" => usage_metaclass(&tokens, kerml),
            "kerml_usage" => kerml_metaclass(first, has("struct"), has("false")),
            "action_usage" => {
                if has("send") {
                    M::SendActionUsage
                } else if has("accept") {
                    M::AcceptActionUsage
                } else if has("assign") {
                    M::AssignmentActionUsage
                } else if has("terminate") {
                    M::TerminateActionUsage
                } else if has("for") {
                    M::ForLoopActionUsage
                } else {
                    M::ActionUsage
                }
            }
            "state_usage" => M::StateUsage,
            "connection_usage" => {
                if has("flow") {
                    M::FlowUsage
                } else {
                    M::ConnectionUsage
                }
            }
            "interface_usage" | "interface_statement" => M::InterfaceUsage,
            "constraint_usage" => {
                if has("assert") {
                    M::AssertConstraintUsage
                } else {
                    M::ConstraintUsage
                }
            }
            "assert_statement" => M::AssertConstraintUsage,
            "requirement_usage" => M::RequirementUsage,
            "satisfy_statement" => M::SatisfyRequirementUsage,
            "event_usage" => M::EventOccurrenceUsage,
            "allocation_usage" | "allocate_statement" => M::AllocationUsage,
            "flow_usage" | "flow_statement" => {
                if kerml {
                    M::Flow
                } else {
                    M::FlowUsage
                }
            }
            "message_statement" => M::FlowUsage,
            "metadata_usage" | "metadata_annotation" => self.metadata_kind(),
            "constraint_expression_usage" => match first {
                "inv" | "invariant" => M::Invariant,
                _ => M::BooleanExpression,
            },
            "binding_usage" | "bind_statement" => {
                if kerml {
                    M::BindingConnector
                } else {
                    M::BindingConnectorAsUsage
                }
            }
            "succession_usage" | "succession_statement" => {
                if kerml {
                    M::Succession
                } else {
                    M::SuccessionAsUsage
                }
            }
            "succession_flow_usage" => {
                if kerml {
                    M::SuccessionFlow
                } else {
                    M::SuccessionFlowUsage
                }
            }
            "connect_statement" => {
                if kerml {
                    M::Connector
                } else {
                    M::ConnectionUsage
                }
            }
            "perform_statement" => M::PerformActionUsage,
            "exhibit_statement" => M::ExhibitStateUsage,
            "include_statement" => M::IncludeUseCaseUsage,
            "transition_statement" => M::TransitionUsage,
            "end_feature" => {
                // end [cross] [keyword] name ...
                let kw = tokens.iter().find(|t| {
                    matches!(
                        **t,
                        "ref"
                            | "item"
                            | "port"
                            | "part"
                            | "attribute"
                            | "feature"
                            | "occurrence"
                            | "action"
                            | "connection"
                            | "flow"
                    )
                });
                match kw.copied() {
                    Some("item") => M::ItemUsage,
                    Some("port") => M::PortUsage,
                    Some("part") => M::PartUsage,
                    Some("attribute") => M::AttributeUsage,
                    Some("occurrence") => M::OccurrenceUsage,
                    Some("action") => M::ActionUsage,
                    Some("connection") => M::ConnectionUsage,
                    Some("flow") => M::FlowUsage,
                    Some("feature") => M::Feature,
                    _ => self.ref_usage_kind(),
                }
            }
            "if_action" => M::IfActionUsage,
            "while_action" | "loop_action" => M::WhileLoopActionUsage,
            "for_action" => M::ForLoopActionUsage,
            "assign_action" => M::AssignmentActionUsage,
            "send_action" => M::SendActionUsage,
            "accept_action" => M::AcceptActionUsage,
            "terminate_statement" => M::TerminateActionUsage,
            "control_node" => match first {
                "merge" => M::MergeNode,
                "decide" => M::DecisionNode,
                "fork" => M::ForkNode,
                _ => M::JoinNode,
            },
            "feature_usage" => {
                if prefix.has("ref") || kerml {
                    self.ref_usage_kind()
                } else {
                    self.ref_usage_kind()
                }
            }
            "redefinition_statement" | "standalone_redefines" => self.ref_usage_kind(),
            _ => return None,
        })
    }

    pub(crate) fn metadata_kind(&self) -> Metaclass {
        if self.kerml {
            M::MetadataFeature
        } else {
            M::MetadataUsage
        }
    }

    // ------------------------------------------------------------------
    // Namespaces, imports, aliases, annotations

    fn package(&mut self, node: Node, prefix: &Prefix) -> ElementRef {
        let ks = kids(node);
        let library = ks.iter().any(|k| k.kind() == "library");
        let kind = if node.kind() == "namespace_declaration" {
            M::Namespace
        } else if library {
            M::LibraryPackage
        } else {
            M::Package
        };
        let span = self.span_from(prefix.start.unwrap_or(node.start_byte()), node);
        let p = self.new_el(kind, span);
        if ks.iter().any(|k| k.kind() == "standard") {
            self.el(p).flags.insert(Flags::STANDARD);
        }
        self.prefix_metadata_members(p, prefix, node);
        for k in &ks {
            match k.kind() {
                "short_name" => self.set_short_name(p, *k),
                "identifier" => self.set_name(p, *k),
                _ => {}
            }
        }
        if let Some(body) = ks.iter().find(|k| k.kind() == "package_body") {
            self.body_as(p, *body, BodyKind::Package);
        }
        p
    }

    /// `PrefixMetadataMember`s for `#X` written before (or inside) `node`.
    fn prefix_metadata_members(&mut self, e: ElementRef, prefix: &Prefix, node: Node) {
        let mut tags: Vec<Node> = prefix.metadata.clone();
        for k in kids(node) {
            if k.kind() == "hash_tag" {
                tags.push(k);
            }
        }
        for tag in tags {
            let mu = self.prefix_metadata_feature(tag);
            self.own_member(e, M::OwningMembership, mu, None);
        }
    }

    /// `#Name`: a metadata feature typed by `Name`.
    fn prefix_metadata_feature(&mut self, tag: Node) -> ElementRef {
        let kind = self.metadata_kind();
        let span = self.span(tag);
        let mu = self.new_el(kind, span);
        if let Some(name) = kids(tag).into_iter().find(|k| k.is_named()) {
            let t = self.rel(mu, M::FeatureTyping, self.span(name));
            self.set_end(t, Role::Target, name);
        }
        mu
    }

    fn import(&mut self, ns: ElementRef, node: Node, prefix: &Prefix) {
        let ks = kids(node);
        let expose = node.kind() == "expose_statement";
        let mut visibility = prefix.visibility;
        let mut all = false;
        let mut name = None;
        let mut filter = None;
        let mut body = None;
        for k in &ks {
            match k.kind() {
                "visibility" => visibility = visibility_of(self.text(*k)),
                "all" => all = true,
                "qualified_name" => name = Some(*k),
                "import_filter" => filter = Some(*k),
                "definition_body" => body = Some(*k),
                _ => {}
            }
        }
        let Some(name) = name else { return };
        // The `::*` and `::**` tokens are hidden in the CST: read the text
        // between the name and what follows it.
        let after_end = ks
            .iter()
            .find(|k| k.start_byte() >= name.end_byte() && k.kind() != "qualified_name")
            .map(|k| k.start_byte())
            .unwrap_or(node.end_byte());
        let tail: String = self.src[name.end_byte()..after_end]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let namespace = tail.starts_with("::*") && !tail.starts_with("::**");
        let recursive = tail.contains("**");
        let (kind_ns, kind_mem) = if expose {
            (M::NamespaceExpose, M::MembershipExpose)
        } else {
            (M::NamespaceImport, M::MembershipImport)
        };
        let span = self.span_from(prefix.start.unwrap_or(node.start_byte()), node);
        let set_flags = |l: &mut Self, r: ElementRef| {
            let el = l.el(r);
            el.flags.set(Flags::RECURSIVE, recursive);
            el.flags.set(Flags::IMPORT_ALL, all);
        };
        let imp = if let Some(filter) = filter {
            // Filtered import: a NamespaceImport of an owned FilterPackage
            // that imports the named elements and filters them.
            let imp = self.rel(ns, kind_ns, span);
            let package = self.new_el(M::Package, self.span(node));
            self.g.attach(imp, package);
            self.el(imp).ends.push(End {
                role: Role::Target,
                target: Target::Owned(package),
            });
            let inner = self.rel(
                package,
                if namespace {
                    M::NamespaceImport
                } else {
                    M::MembershipImport
                },
                self.span(name),
            );
            set_flags(self, inner);
            self.set_end(inner, Role::Target, name);
            self.el(inner).visibility = Visibility::Public;
            for f in kids(filter) {
                if f.kind() == "filter_expression" {
                    let x = self.filter_expression(f);
                    self.own_member(package, M::ElementFilterMembership, x, None);
                }
            }
            imp
        } else {
            let imp = self.rel(ns, if namespace { kind_ns } else { kind_mem }, span);
            set_flags(self, imp);
            self.set_end(imp, Role::Target, name);
            imp
        };
        let el = self.el(imp);
        el.visibility = visibility.unwrap_or(if expose {
            Visibility::Public
        } else {
            Visibility::Private
        });
        if visibility.is_some() {
            el.flags.insert(Flags::VISIBILITY_WRITTEN);
        }
        if let Some(body) = body {
            self.relationship_body(imp, body);
        }
    }

    /// `RelationshipBody`: annotations owned by a relationship.
    fn relationship_body(&mut self, rel: ElementRef, body: Node) {
        for k in all_kids(body) {
            let a = match k.kind() {
                "doc_comment" => self.documentation(k),
                "comment_element" => {
                    let c = self.new_el(M::Comment, self.span(k));
                    self.comment_parts(c, k);
                    c
                }
                "block_comment" if self.text(k).starts_with("/*") => {
                    let c = self.new_el(M::Comment, self.span(k));
                    let body = self.comment_body(k);
                    self.set_body(c, body, None);
                    c
                }
                "metadata_annotation" | "metadata_usage" => {
                    self.metadata_usage(k, &Prefix::default())
                }
                "textual_representation" => self.textual_representation(k),
                _ => continue,
            };
            let an = self.rel(rel, M::Annotation, self.span(k));
            self.g.attach(an, a);
        }
    }

    fn alias(&mut self, ns: ElementRef, node: Node, prefix: &Prefix) {
        let span = self.span_from(prefix.start.unwrap_or(node.start_byte()), node);
        let m = self.rel(ns, M::Membership, span);
        let mut visibility = prefix.visibility;
        for k in kids(node) {
            match k.kind() {
                "visibility" => visibility = visibility_of(self.text(k)),
                "short_name" => self.set_short_name(m, k),
                "identifier" if node.child_by_field_name("name") == Some(k) => self.set_name(m, k),
                "qualified_name" => self.set_end(m, Role::Target, k),
                "definition_body" => self.relationship_body(m, k),
                _ => {}
            }
        }
        if let Some(v) = visibility {
            let el = self.el(m);
            el.visibility = v;
            el.flags.insert(Flags::VISIBILITY_WRITTEN);
        }
    }

    pub(crate) fn comment(&mut self, ns: ElementRef, node: Node, visibility: Option<Visibility>) {
        let c = self.new_el(M::Comment, self.span(node));
        if node.kind() == "block_comment" {
            let body = self.comment_body(node);
            self.set_body(c, body, None);
        } else {
            self.comment_parts(c, node);
        }
        self.own_member(ns, M::OwningMembership, c, visibility);
    }

    fn comment_parts(&mut self, c: ElementRef, node: Node) {
        let mut locale = None;
        let mut body = None;
        let mut about = false;
        for k in all_kids(node) {
            match k.kind() {
                "short_name" => self.set_short_name(c, k),
                "identifier" => self.set_name(c, k),
                "about" => about = true,
                "locale" => about = false,
                "qualified_name" | "feature_chain" if about => {
                    let a = self.rel(c, M::Annotation, self.span(k));
                    self.set_end(a, Role::Target, k);
                }
                "string_literal" => locale = Some(self.string_value(k)),
                "block_comment" => body = Some(self.comment_body(k)),
                _ => {}
            }
        }
        self.set_body(c, body.unwrap_or_default(), locale);
    }

    pub(crate) fn documentation(&mut self, node: Node) -> ElementRef {
        let d = self.new_el(M::Documentation, self.span(node));
        let mut locale = None;
        let mut body = None;
        for k in all_kids(node) {
            match k.kind() {
                "short_name" => self.set_short_name(d, k),
                "identifier" => self.set_name(d, k),
                "string_literal" => locale = Some(self.string_value(k)),
                "block_comment" => body = Some(self.comment_body(k)),
                _ => {}
            }
        }
        self.set_body(d, body.unwrap_or_default(), locale);
        d
    }

    fn textual_representation(&mut self, node: Node) -> ElementRef {
        let r = self.new_el(M::TextualRepresentation, self.span(node));
        let mut language = None;
        let mut body = None;
        for k in all_kids(node) {
            match k.kind() {
                "identifier" => self.set_name(r, k),
                "string_literal" => language = Some(self.string_value(k)),
                "block_comment" => body = Some(self.comment_body(k)),
                _ => {}
            }
        }
        self.el(r).value = Value::Body {
            body: body.unwrap_or_default(),
            locale: None,
            language,
        };
        r
    }

    fn dependency(&mut self, node: Node, prefix: &Prefix) -> ElementRef {
        let span = self.span_from(prefix.start.unwrap_or(node.start_byte()), node);
        let d = self.new_el(M::Dependency, span);
        // PrefixMetadataAnnotations.
        let mut tags = prefix.metadata.clone();
        tags.extend(kids(node).into_iter().filter(|k| k.kind() == "hash_tag"));
        for tag in tags {
            let mu = self.prefix_metadata_feature(tag);
            let a = self.rel(d, M::Annotation, self.span(tag));
            self.g.attach(a, mu);
        }
        let ks = kids(node);
        let has_from = ks.iter().any(|k| k.kind() == "from");
        let mut role = if has_from { Role::Source } else { Role::Source };
        let mut seen_from = false;
        for k in ks {
            match k.kind() {
                "identifier" if node.child_by_field_name("name") == Some(k) => self.set_name(d, k),
                "from" => seen_from = true,
                "to" => role = Role::Target,
                "qualified_name" | "feature_chain" => {
                    let _ = seen_from;
                    let target = Target::Name(self.reference(d, k));
                    self.el(d).ends.push(End { role, target });
                }
                "definition_body" => self.relationship_body(d, k),
                _ => {}
            }
        }
        d
    }

    /// KerML standalone relationships (`specialization`, `subclassifier`,
    /// `subtype`, `subset`, `redefinition`, `typing`, `conjugation`,
    /// `featuring`, `disjoining`, `inverting`).
    fn standalone_relationship(&mut self, node: Node) -> ElementRef {
        let ks = kids(node);
        let kw: Vec<&str> = ks
            .iter()
            .filter(|k| !k.is_named())
            .map(|k| k.kind())
            .collect();
        let kind = match node.kind() {
            "conjugation_statement" => M::Conjugation,
            "featuring_statement" => M::TypeFeaturing,
            "disjoining_statement" => M::Disjoining,
            "inverse_statement" => M::FeatureInverting,
            _ => {
                if kw.contains(&"subclassifier") {
                    M::Subclassification
                } else if kw.contains(&"subset") {
                    M::Subsetting
                } else if kw.contains(&"redefinition") {
                    M::Redefinition
                } else if kw.contains(&"typing") {
                    M::FeatureTyping
                } else {
                    M::Specialization
                }
            }
        };
        let r = self.new_el(kind, self.span(node));
        // Name: `specialization Name subtype ...`; for the other forms the
        // first identifier after the leading keyword (if followed by another
        // keyword) is the name.
        let mut refs: Vec<Node> = Vec::new();
        for k in &ks {
            match k.kind() {
                "identifier" if node.child_by_field_name("name") == Some(*k) => {
                    if kind == M::Subclassification && !kw.contains(&"specialization") {
                        // `subclassifier A specializes B`: A is the subclassifier.
                        refs.push(*k);
                    } else {
                        self.set_name(r, *k)
                    }
                }
                "qualified_name" | "feature_chain" => refs.push(*k),
                "keyword_type_relationship" | "typed_by" | "specialization" | "redefinition" => {
                    if let Some(t) = kids(*k).into_iter().find(|t| t.is_named()) {
                        refs.push(t);
                    }
                }
                "typed_by_keyword" => {
                    if let Some(t) = kids(*k).into_iter().find(|t| t.is_named()) {
                        refs.push(t);
                    }
                }
                _ => {}
            }
        }
        if let Some(source) = refs.first() {
            self.set_end(r, Role::Source, *source);
        }
        for target in refs.iter().skip(1) {
            self.set_end(r, Role::Target, *target);
        }
        r
    }

    /// `multiplicity m [1..2];` / `multiplicity m :> exactlyOne;`
    fn multiplicity_member_definition(&mut self, node: Node, prefix: &Prefix) -> ElementRef {
        let ks = kids(node);
        let range = ks.iter().find(|k| k.kind() == "multiplicity");
        let m = if let Some(range) = range {
            let m = self.new_el(M::MultiplicityRange, self.span(node));
            self.multiplicity_bounds(m, *range);
            m
        } else {
            self.new_el(M::Multiplicity, self.span(node))
        };
        self.prefix_metadata_members(m, prefix, node);
        for k in &ks {
            match k.kind() {
                "identifier" => self.set_name(m, *k),
                "specialization" | "keyword_type_relationship" | "redefinition" => {
                    self.type_relationship(m, *k)
                }
                "definition_body" => self.body_as(m, *k, BodyKind::Type),
                _ => {}
            }
        }
        m
    }

    // ------------------------------------------------------------------
    // Declarations

    /// Create the element for a declaration node of metaclass `meta` and
    /// lower its parts.
    pub(crate) fn declaration(
        &mut self,
        node: Node,
        meta: Metaclass,
        prefix: &Prefix,
        _ctx: BodyKind,
    ) -> ElementRef {
        let span = self.span_from(prefix.start.unwrap_or(node.start_byte()), node);
        let e = self.new_el(meta, span);
        self.apply_modifiers(e, prefix);
        let ks = kids(node);
        // Flags written as tokens of the node itself.
        for k in &ks {
            match k.kind() {
                "modifier" => {
                    let m = token_of(*k);
                    self.apply_modifier(e, m);
                }
                "parallel" => self.el(e).flags.insert(Flags::PARALLEL),
                _ => {}
            }
        }
        if meta.is_occurrence_definition() && prefix.has("individual") {
            self.el(e).flags.insert(Flags::INDIVIDUAL);
        }
        // `snapshot` / `timeslice` portion usages.
        for k in &ks {
            match k.kind() {
                "snapshot" => {
                    self.el(e).value = Value::Portion(PortionKind::Snapshot);
                    self.el(e).flags.insert(Flags::PORTION);
                }
                "timeslice" => {
                    self.el(e).value = Value::Portion(PortionKind::Timeslice);
                    self.el(e).flags.insert(Flags::PORTION);
                }
                _ => {}
            }
        }
        // End usage cross feature: `end name [mult] kw name ...`.
        if node.kind() == "end_feature" {
            self.el(e).flags.insert(Flags::END);
            self.end_cross_feature(e, node);
        }
        self.prefix_metadata_members(e, prefix, node);
        // OccurrenceDefinitionPrefix: `individual` adds an empty multiplicity
        // before the declaration.
        if meta.is_occurrence_definition()
            && prefix.has("individual")
            && node.kind() == "definition"
        {
            let mult = self.new_el(M::Multiplicity, span);
            self.own_member(e, M::OwningMembership, mult, None);
        }
        match node.kind() {
            "perform_statement" | "exhibit_statement" | "include_statement" => {
                self.referencing_usage(e, node)
            }
            "event_usage" => self.event_usage(e, node),
            "satisfy_statement" => self.satisfy(e, node),
            "assert_statement" => self.assert_statement(e, node),
            "connect_statement"
            | "connection_usage"
            | "interface_usage"
            | "allocation_usage"
            | "allocate_statement"
            | "interface_statement" => self.connection(e, node),
            "binding_usage" | "bind_statement" => self.binding(e, node),
            "succession_usage" | "succession_statement" => self.succession(e, node),
            "flow_usage" | "flow_statement" | "succession_flow_usage" | "message_statement" => {
                self.flow(e, node)
            }
            "kerml_usage" if meta.is_connector() => self.kerml_connector(e, node),
            "action_usage"
            | "send_action"
            | "accept_action"
            | "assign_action"
            | "if_action"
            | "while_action"
            | "loop_action"
            | "for_action"
            | "terminate_statement" => self.action_node(e, node),
            "transition_statement" => self.transition_parts(e, node),
            "metadata_usage" | "metadata_annotation" => self.metadata_parts(e, node),
            "end_feature" => {}
            "redefinition_statement" => self.redefinition_statement(e, node),
            "standalone_redefines" => self.standalone_redefines(e, node),
            _ => self.decl_parts(e, &ks),
        }
        // A PortDefinition owns its ConjugatedPortDefinition (after the body).
        if meta == M::PortDefinition {
            let c = self.new_el(M::ConjugatedPortDefinition, span);
            let pc = self.rel(c, M::PortConjugation, span);
            self.el(pc).ends.push(End {
                role: Role::Target,
                target: Target::Element(e),
            });
            self.own_member(e, M::OwningMembership, c, None);
        }
        // `individual def X`: the empty multiplicity follows the definition.
        if node.kind() == "definition" && kids(node).first().map(|k| k.kind()) == Some("individual")
        {
            self.el(e).flags.insert(Flags::INDIVIDUAL);
            let mult = self.new_el(M::Multiplicity, span);
            self.own_member(e, M::OwningMembership, mult, None);
        }
        e
    }

    fn apply_modifiers(&mut self, e: ElementRef, prefix: &Prefix) {
        for m in &prefix.modifiers {
            self.apply_modifier(e, m);
        }
    }

    fn apply_modifier(&mut self, e: ElementRef, m: &str) {
        let kerml = self.kerml;
        let el = self.el(e);
        match m {
            "abstract" => el.flags.insert(Flags::ABSTRACT),
            "variation" => el.flags.insert(Flags::VARIATION),
            "variant" => el.flags.insert(Flags::VARIANT),
            "individual" => el.flags.insert(Flags::INDIVIDUAL),
            "derived" => el.flags.insert(Flags::DERIVED),
            "readonly" | "constant" => el.flags.insert(Flags::CONSTANT),
            "const" => {
                el.flags.insert(Flags::CONSTANT);
                if kerml {
                    el.flags.insert(Flags::VARIABLE);
                }
            }
            "nonunique" => el.flags.insert(Flags::NONUNIQUE),
            "ordered" => el.flags.insert(Flags::ORDERED),
            "in" => el.direction = Some(Direction::In),
            "out" => el.direction = Some(Direction::Out),
            "inout" => el.direction = Some(Direction::InOut),
            "composite" => el.flags.insert(Flags::COMPOSITE),
            "portion" => el.flags.insert(Flags::PORTION),
            "var" => el.flags.insert(Flags::VARIABLE),
            "ref" => el.flags.insert(Flags::REFERENCE),
            _ => {}
        }
    }

    /// The common declaration parts: identification, specializations,
    /// multiplicity, value and body, in textual order.
    pub(crate) fn decl_parts(&mut self, e: ElementRef, ks: &[Node]) {
        for k in ks {
            self.decl_part(e, *k);
        }
    }

    /// Lower one part of a declaration; returns whether it was recognised.
    pub(crate) fn decl_part(&mut self, e: ElementRef, k: Node) -> bool {
        match k.kind() {
            "short_name" => self.set_short_name(e, k),
            "identifier" => {
                // Only the `name:` field is the declared name.
                let parent = k.parent();
                let is_name = parent
                    .and_then(|p| p.child_by_field_name("name"))
                    .map(|n| n == k)
                    .unwrap_or(false);
                if is_name {
                    self.set_name(e, k);
                } else {
                    return false;
                }
            }
            "multiplicity" => self.owned_multiplicity(e, k),
            "nonunique" => self.el(e).flags.insert(Flags::NONUNIQUE),
            "ordered" => self.el(e).flags.insert(Flags::ORDERED),
            "all" => self.el(e).flags.insert(Flags::SUFFICIENT),
            "typed_by"
            | "typed_by_keyword"
            | "specialization"
            | "redefinition"
            | "keyword_type_relationship"
            | "binding"
            | "inverse_keyword"
            | "tilde_conjugation" => self.type_relationship(e, k),
            "value_assignment" => self.feature_value(e, k),
            "metadata_annotation_list" => {
                for t in kids(k) {
                    if matches!(t.kind(), "qualified_name" | "feature_chain") {
                        let mu = self.new_el(self.metadata_kind(), self.span(t));
                        let ty = self.rel(mu, M::FeatureTyping, self.span(t));
                        self.set_end(ty, Role::Target, t);
                        self.own_member(e, M::OwningMembership, mu, None);
                    }
                }
            }
            b if BODIES.contains(&b) => self.body(e, k),
            _ => return false,
        }
        true
    }

    /// The `OwnedMultiplicity` of a feature (and its ordering flags).
    pub(crate) fn owned_multiplicity(&mut self, e: ElementRef, node: Node) {
        let m = self.new_el(M::MultiplicityRange, self.span(node));
        self.multiplicity_bounds(m, node);
        self.own_member(e, M::OwningMembership, m, None);
        for k in kids(node) {
            match k.kind() {
                "ordered" => self.el(e).flags.insert(Flags::ORDERED),
                "nonunique" => self.el(e).flags.insert(Flags::NONUNIQUE),
                _ => {}
            }
        }
    }

    /// `[lower..upper]` bounds as `MultiplicityExpressionMember`s.
    pub(crate) fn multiplicity_bounds(&mut self, m: ElementRef, node: Node) {
        for k in kids(node) {
            let x = match k.kind() {
                "*" => {
                    let x = self.new_el(M::LiteralInfinity, self.span(k));
                    Some(x)
                }
                "[" | "]" | ".." | "ordered" | "nonunique" => None,
                _ if k.is_named() => Some(self.expression(k)),
                _ => None,
            };
            if let Some(x) = x {
                self.own_member(m, M::OwningMembership, x, None);
            }
        }
    }

    /// A specialization-like part of a declaration.
    pub(crate) fn type_relationship(&mut self, e: ElementRef, node: Node) {
        let kind = self.kind_of(e);
        let targets: Vec<Node> = kids(node)
            .into_iter()
            .filter(|t| matches!(t.kind(), "qualified_name" | "feature_chain" | "identifier"))
            .collect();
        let rel_kind = match node.kind() {
            "typed_by" | "typed_by_keyword" => {
                let conjugated = kids(node).iter().any(|t| t.kind() == "~");
                if conjugated {
                    M::ConjugatedPortTyping
                } else {
                    M::FeatureTyping
                }
            }
            "specialization" => self.specializes_kind(kind),
            "redefinition" => M::Redefinition,
            "binding" => M::ReferenceSubsetting,
            "inverse_keyword" => M::FeatureInverting,
            "tilde_conjugation" => M::Conjugation,
            "keyword_type_relationship" => {
                let first = node.child(0).map(|c| c.kind()).unwrap_or("");
                match first {
                    "redefines" => M::Redefinition,
                    "subsets" => M::Subsetting,
                    "references" => M::ReferenceSubsetting,
                    "crosses" | "=>" => M::CrossSubsetting,
                    "specializes" => self.specializes_kind(kind),
                    "conjugates" => M::Conjugation,
                    "chains" => {
                        // `chains a.b.c`: the feature itself is the chain.
                        for t in &targets {
                            self.chainings(e, *t);
                        }
                        return;
                    }
                    "unions" => M::Unioning,
                    "intersects" => M::Intersecting,
                    "differences" => M::Differencing,
                    "disjoint" => M::Disjoining,
                    "featuring" | "featured" => M::TypeFeaturing,
                    _ => return,
                }
            }
            _ => return,
        };
        // Typed-by targets are always qualified names; for a typing written
        // `: ~T` the conjugated port typing points at T's conjugate.
        for t in targets {
            let r = self.rel(e, rel_kind, self.span(t));
            self.set_end(r, Role::Target, t);
        }
    }

    /// The relationship `:>` / `specializes` produces for `kind`.
    fn specializes_kind(&self, kind: Metaclass) -> Metaclass {
        if kind.is_classifier() {
            M::Subclassification
        } else if kind.is_feature() {
            M::Subsetting
        } else {
            M::Specialization
        }
    }

    /// `FeatureValue`.
    pub(crate) fn feature_value(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let fv = self.rel(e, M::FeatureValue, self.span(node));
        let mut seen_default = false;
        for k in &ks {
            match k.kind() {
                "default" => {
                    seen_default = true;
                    self.el(fv).flags.insert(Flags::DEFAULT);
                }
                ":=" => self.el(fv).flags.insert(Flags::INITIAL),
                _ => {}
            }
        }
        let _ = seen_default;
        if let Some(x) = ks.iter().rev().find(|k| k.is_named()) {
            let ex = if x.kind() == "definition_body" {
                self.body_expression(*x)
            } else {
                self.expression(*x)
            };
            self.g.attach(fv, ex);
        }
    }

    /// `end [crossName] [mult] [specializations] keyword name ...`: an owned
    /// cross feature (KerML `OwnedCrossFeatureMember`, SysML 8.2.2.6.2).
    ///
    /// The grammar does not parse cross features with specializations
    /// before the keyword (`end happensWhile [1..*] subsets x feature y`):
    /// the keyword and end name land in an `ERROR` node, which is read here.
    fn end_cross_feature(&mut self, e: ElementRef, node: Node) {
        const KEYWORDS: &[&str] = &[
            "ref",
            "item",
            "port",
            "part",
            "attribute",
            "feature",
            "occurrence",
            "action",
            "connection",
            "flow",
        ];
        let ks = kids(node);
        let end_name = node.child_by_field_name("end_name");
        let src = self.src;
        let is_kw = |k: &Node| {
            KEYWORDS.contains(&k.kind())
                || (k.kind() == "ERROR"
                    && src[k.start_byte()..k.end_byte()]
                        .split_whitespace()
                        .next()
                        .map(|w| KEYWORDS.contains(&w))
                        .unwrap_or(false))
        };
        let kw_index = ks.iter().position(is_kw);
        let name = node.child_by_field_name("name");
        // Name of the end feature from an ERROR node (`feature name`).
        let error_name: Option<(Box<str>, (u32, u32))> =
            kw_index.filter(|i| ks[*i].kind() == "ERROR").and_then(|i| {
                let err = ks[i];
                let text = self.text(err);
                let mut words = text.split_whitespace();
                let _keyword = words.next()?;
                let word = words.next()?;
                let offset = err.start_byte() + text.find(word)?;
                Some((
                    crate::name::unescape_name(word).into(),
                    (offset as u32, (offset + word.len()) as u32),
                ))
            });
        // Where the end feature's own declaration starts.
        let split = match (end_name, kw_index) {
            (Some(_), Some(i)) => Some(i),
            (Some(_), None) => name.and_then(|n| ks.iter().position(|k| *k == n)),
            (None, Some(i)) => Some(i),
            (None, None) => None,
        };
        let cross_parts: Vec<Node> = match (end_name, split) {
            (Some(en), Some(split)) => {
                let start = ks.iter().position(|k| *k == en).map(|i| i + 1).unwrap_or(0);
                ks[start..split].to_vec()
            }
            (None, Some(split)) => {
                // `end [mult] feature x`: only a multiplicity.
                ks[1..split]
                    .iter()
                    .copied()
                    .filter(|k| k.kind() == "multiplicity")
                    .collect()
            }
            _ => Vec::new(),
        };
        let has_cross = (end_name.is_some() && split.is_some()) || !cross_parts.is_empty();
        if has_cross {
            let kind = self.ref_usage_kind();
            let span = self.span(node);
            let cf = self.new_el(kind, span);
            if let (Some(n), Some(_)) = (end_name, split) {
                self.set_name(cf, n);
            }
            for k in &cross_parts {
                self.decl_part(cf, *k);
            }
            self.own_member(e, M::OwningMembership, cf, None);
        }
        let rest_start = match split {
            Some(i) if has_cross => i,
            _ => 0,
        };
        for k in &ks[rest_start..] {
            if Some(*k) == end_name {
                // `end name : T` with no keyword: the end's declared name.
                if !has_cross {
                    self.set_name(e, *k);
                }
                continue;
            }
            if k.kind() == "ERROR" {
                if let Some((n, span)) = error_name.clone() {
                    let el = self.el(e);
                    el.name = Some(n);
                    el.name_span = Some(span);
                }
                continue;
            }
            self.decl_part(e, *k);
        }
    }

    // ------------------------------------------------------------------
    // SysML statements that reference an existing feature

    /// `perform x;` / `exhibit x;` / `include x;` / `perform action x : T;`:
    /// with the declaration keyword the name is declared; otherwise the
    /// qualified name is an owned reference subsetting.
    fn referencing_usage(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let declared = ks
            .iter()
            .any(|k| matches!(k.kind(), "action" | "state" | "case"));
        let mut first_ref = true;
        for k in ks {
            match k.kind() {
                "qualified_name" | "feature_chain" if first_ref => {
                    first_ref = false;
                    if declared && k.kind() == "qualified_name" {
                        self.set_name(e, k);
                    } else {
                        let r = self.rel(e, M::ReferenceSubsetting, self.span(k));
                        self.set_end(r, Role::Target, k);
                    }
                }
                _ => {
                    self.decl_part(e, k);
                }
            }
        }
    }

    /// `event x;` (reference subsetting) or `event occurrence x : T;`.
    fn event_usage(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let declared = ks.iter().any(|k| k.kind() == "occurrence");
        let mut first_ref = true;
        for k in ks {
            match k.kind() {
                "qualified_name" | "feature_chain" if first_ref => {
                    first_ref = false;
                    if declared && k.kind() == "qualified_name" {
                        self.set_name(e, k);
                    } else {
                        let r = self.rel(e, M::ReferenceSubsetting, self.span(k));
                        self.set_end(r, Role::Target, k);
                    }
                }
                _ => {
                    self.decl_part(e, k);
                }
            }
        }
    }

    /// `[assert] [not] satisfy [requirement] x [: T] [by y]`.
    fn satisfy(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        if ks.iter().any(|k| k.kind() == "not") {
            self.el(e).flags.insert(Flags::NEGATED);
        }
        let declared = ks.iter().any(|k| k.kind() == "requirement");
        let mut first_ref = true;
        let mut after_by = false;
        for k in ks {
            match k.kind() {
                "by" => after_by = true,
                "qualified_name" | "feature_chain" if after_by => {
                    // SatisfactionSubjectMember: a subject parameter bound to
                    // the satisfying feature.
                    let kind = self.ref_usage_kind();
                    let p = self.new_el(kind, self.span(k));
                    let fv = self.rel(p, M::FeatureValue, self.span(k));
                    let x = self.feature_reference_expression(k);
                    self.g.attach(fv, x);
                    self.own_member(e, M::SubjectMembership, p, None);
                }
                "qualified_name" | "feature_chain" if first_ref => {
                    first_ref = false;
                    if declared && k.kind() == "qualified_name" {
                        self.set_name(e, k);
                    } else {
                        let r = self.rel(e, M::ReferenceSubsetting, self.span(k));
                        self.set_end(r, Role::Target, k);
                    }
                }
                _ => {
                    self.decl_part(e, k);
                }
            }
        }
    }

    /// `assert [not] x { ... }`.
    fn assert_statement(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        if ks.iter().any(|k| k.kind() == "not") {
            self.el(e).flags.insert(Flags::NEGATED);
        }
        let mut first_ref = true;
        for k in ks {
            match k.kind() {
                "qualified_name" | "feature_chain" if first_ref => {
                    first_ref = false;
                    let r = self.rel(e, M::ReferenceSubsetting, self.span(k));
                    self.set_end(r, Role::Target, k);
                }
                _ => {
                    self.decl_part(e, k);
                }
            }
        }
    }

    /// A usage that is either a reference to an existing feature (owned
    /// reference subsetting) or a declaration (`frame concern c;`).
    fn reference_or_declaration(
        &mut self,
        node: Node,
        meta: Metaclass,
        prefix: &Prefix,
    ) -> ElementRef {
        let span = self.span_from(prefix.start.unwrap_or(node.start_byte()), node);
        let e = self.new_el(meta, span);
        self.apply_modifiers(e, prefix);
        self.prefix_metadata_members(e, prefix, node);
        let ks = kids(node);
        let declared = ks.iter().any(|k| {
            matches!(
                k.kind(),
                "concern" | "requirement" | "rendering" | "constraint"
            )
        }) || node.child_by_field_name("name").map(|n| n.kind())
            == Some("identifier");
        let mut first_ref = true;
        for k in ks {
            match k.kind() {
                "qualified_name" | "feature_chain" if first_ref && !declared => {
                    first_ref = false;
                    let r = self.rel(e, M::ReferenceSubsetting, self.span(k));
                    self.set_end(r, Role::Target, k);
                }
                "qualified_name" if first_ref && declared => {
                    first_ref = false;
                    self.set_name(e, k);
                }
                _ => {
                    self.decl_part(e, k);
                }
            }
        }
        e
    }

    /// `require constraint c { ... }` / `require c;` / `assume ...`.
    fn requirement_constraint(
        &mut self,
        ns: ElementRef,
        node: Node,
        prefix: &Prefix,
        kind: &'static str,
    ) {
        let ks = kids(node);
        let span = self.span_from(prefix.start.unwrap_or(node.start_byte()), node);
        let meta = if ks.iter().any(|k| k.kind() == "requirement") {
            M::RequirementUsage
        } else {
            M::ConstraintUsage
        };
        let e = self.new_el(meta, span);
        self.apply_modifiers(e, prefix);
        self.prefix_metadata_members(e, prefix, node);
        let declared = ks
            .iter()
            .any(|k| matches!(k.kind(), "constraint" | "requirement"));
        let name = node.child_by_field_name("name");
        for k in ks {
            if Some(k) == name {
                if declared && k.kind() == "qualified_name" {
                    self.set_name(e, k);
                } else if declared && k.kind() == "identifier" {
                    self.set_name(e, k);
                } else {
                    let r = self.rel(e, M::ReferenceSubsetting, self.span(k));
                    self.set_end(r, Role::Target, k);
                }
                continue;
            }
            self.decl_part(e, k);
        }
        let m = self.own_member(ns, M::RequirementConstraintMembership, e, prefix.visibility);
        self.el(m).value = Value::Kind(kind);
    }

    /// `metadata m : M about x { ... }` / `@M { ... }`.
    fn metadata_usage(&mut self, node: Node, prefix: &Prefix) -> ElementRef {
        let span = self.span_from(prefix.start.unwrap_or(node.start_byte()), node);
        let kind = self.metadata_kind();
        let e = self.new_el(kind, span);
        self.apply_modifiers(e, prefix);
        self.prefix_metadata_members(e, prefix, node);
        self.metadata_parts(e, node);
        e
    }

    fn metadata_parts(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let has_typing = ks
            .iter()
            .any(|k| matches!(k.kind(), "typed_by" | "typed_by_keyword"));
        let mut about = false;
        for k in ks {
            match k.kind() {
                "about" => about = true,
                "qualified_name" | "feature_chain" if about => {
                    let a = self.rel(e, M::Annotation, self.span(k));
                    self.set_end(a, Role::Target, k);
                }
                "qualified_name" | "feature_chain" if !has_typing => {
                    // `@M` / `metadata M`: the metaclass.
                    let t = self.rel(e, M::FeatureTyping, self.span(k));
                    self.set_end(t, Role::Target, k);
                }
                "metadata_body" => self.body_as(e, k, BodyKind::Metadata),
                _ => {
                    self.decl_part(e, k);
                }
            }
        }
    }

    /// `:>> x = v { ... }` in a metadata body; also `x = v;`.
    fn metadata_body_usage(&mut self, node: Node, prefix: &Prefix) -> ElementRef {
        let span = self.span_from(prefix.start.unwrap_or(node.start_byte()), node);
        let kind = self.ref_usage_kind();
        let e = self.new_el(kind, span);
        let ks = kids(node);
        let mut first = true;
        for k in ks {
            match k.kind() {
                "qualified_name" | "feature_chain" | "identifier" if first => {
                    first = false;
                    let r = self.rel(e, M::Redefinition, self.span(k));
                    self.set_end(r, Role::Target, k);
                }
                "metadata_body" => self.body_as(e, k, BodyKind::Metadata),
                "definition_body" => self.body_as(e, k, BodyKind::Metadata),
                _ => {
                    self.decl_part(e, k);
                }
            }
        }
        e
    }

    /// `name :>> x ...;` / `:>> x ...;` written without a keyword.
    fn redefinition_statement(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let redefines = ks.iter().any(|k| k.kind() == ">>");
        let mut first_target = true;
        for k in ks {
            match k.kind() {
                "qualified_name" | "feature_chain" if first_target => {
                    first_target = false;
                    let kind = if redefines {
                        M::Redefinition
                    } else {
                        M::Subsetting
                    };
                    let r = self.rel(e, kind, self.span(k));
                    self.set_end(r, Role::Target, k);
                }
                _ => {
                    self.decl_part(e, k);
                }
            }
        }
    }

    /// `redefines x = v;` / `:>> x, y : T;`.
    fn standalone_redefines(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let mut in_targets = true;
        for k in ks {
            match k.kind() {
                "redefines" | ":>>" | "," => {}
                "qualified_name" | "feature_chain" if in_targets => {
                    let r = self.rel(e, M::Redefinition, self.span(k));
                    self.set_end(r, Role::Target, k);
                }
                _ => {
                    in_targets = false;
                    self.decl_part(e, k);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Connectors

    /// A connector end (`ConnectorEndMember`): an end feature with an owned
    /// reference subsetting of the connected feature.
    pub(crate) fn connector_end(
        &mut self,
        conn: ElementRef,
        target: Node,
        name: Option<Node>,
        mult: Option<Node>,
    ) -> ElementRef {
        let kind = if self.kind_of(conn) == M::InterfaceUsage {
            M::PortUsage
        } else {
            self.ref_usage_kind()
        };
        let end = self.new_el(kind, self.span(target));
        self.el(end).flags.insert(Flags::END);
        if let Some(m) = mult {
            // OwnedCrossMultiplicityMember.
            let cross = self.new_el(M::Feature, self.span(m));
            self.owned_multiplicity(cross, m);
            self.own_member(end, M::OwningMembership, cross, None);
        }
        if let Some(n) = name {
            self.set_name(end, n);
        }
        let r = self.rel(end, M::ReferenceSubsetting, self.span(target));
        self.set_end(r, Role::Target, target);
        self.own_member(conn, M::EndFeatureMembership, end, None);
        end
    }

    /// `connect a to b`, `connect (a, b, c)`, `interface a to b`,
    /// `allocate a to b`, `connection c : C connect ...`.
    fn connection(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        for k in ks.iter() {
            match k.kind() {
                "connect_clause" | "allocate_clause" => self.connector_part(e, *k),
                _ => {}
            }
        }
        // Inline `connect`/`allocate` forms on the node itself.
        let inline = matches!(
            node.kind(),
            "connect_statement" | "allocate_statement" | "interface_statement"
        ) || (node.kind() == "interface_usage"
            && ks.iter().any(|k| k.kind() == "to")
            && !ks.iter().any(|k| k.kind() == "connect_clause"));
        if inline {
            self.connector_part(e, node);
            for k in ks {
                if matches!(k.kind(), "short_name")
                    || BODIES.contains(&k.kind())
                    || (k.kind() == "identifier" && node.child_by_field_name("name") == Some(k))
                {
                    self.decl_part(e, k);
                }
            }
            return;
        }
        for k in ks {
            if matches!(k.kind(), "connect_clause" | "allocate_clause") {
                continue;
            }
            self.decl_part(e, k);
        }
    }

    /// The ends of a binary or n-ary connector part.
    fn connector_part(&mut self, conn: ElementRef, node: Node) {
        let ks = kids(node);
        self.connector_ends(conn, &ks);
    }

    /// Connector ends from a token sequence: `[mult] [name references]
    /// target [specializations]`, repeated. `name ::> target` and
    /// `name references target` declare the end's name (KerML 8.2.5.5.1
    /// `ConnectorEnd`).
    pub(crate) fn connector_ends(&mut self, conn: ElementRef, ks: &[Node]) {
        let mut pending_mult: Option<Node> = None;
        let mut pending_name: Option<Node> = None;
        let mut i = 0;
        while i < ks.len() {
            let k = ks[i];
            match k.kind() {
                "multiplicity" => pending_mult = Some(k),
                "identifier" => pending_name = Some(k),
                "qualified_name" | "feature_chain" => {
                    // A following `references x` / `::> x` makes this the name.
                    let next = ks.get(i + 1).copied();
                    let named_target = next.and_then(|n| match n.kind() {
                        "binding" => kids(n).into_iter().find(|t| t.is_named()),
                        "keyword_type_relationship"
                            if n.child(0).map(|c| c.kind()) == Some("references") =>
                        {
                            kids(n).into_iter().find(|t| t.is_named())
                        }
                        _ => None,
                    });
                    match named_target {
                        Some(target) if k.kind() == "qualified_name" => {
                            self.connector_end(conn, target, Some(k), pending_mult.take());
                            pending_name = None;
                            i += 2;
                            continue;
                        }
                        _ => {
                            self.connector_end(conn, k, pending_name.take(), pending_mult.take());
                        }
                    }
                }
                "binding" => {
                    // `name ::> target` where the name was an identifier.
                    if let (Some(name), Some(target)) = (
                        pending_name.take(),
                        kids(k).into_iter().find(|t| t.is_named()),
                    ) {
                        self.connector_end(conn, target, Some(name), pending_mult.take());
                    }
                }
                "typed_by" | "specialization" | "redefinition" | "keyword_type_relationship" => {
                    // `connect a : T to b`: specializations of the end.
                    if let Some(end) = self.last_end(conn) {
                        self.type_relationship(end, k);
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn last_end(&self, conn: ElementRef) -> Option<ElementRef> {
        self.g
            .get(conn)
            .owned
            .iter()
            .rev()
            .find(|m| self.g.get(**m).kind == M::EndFeatureMembership)
            .and_then(|m| self.g.get(*m).owned.first().copied())
    }

    /// KerML `connector c : T from a to b;` / `connector (a, b)`.
    fn kerml_connector(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let has_ends = ks.iter().any(|k| matches!(k.kind(), "to" | "="));
        if !has_ends {
            self.decl_parts(e, &ks);
            return;
        }
        let first_end = ks
            .iter()
            .position(|k| k.kind() == "from")
            .map(|i| i + 1)
            .unwrap_or_else(|| {
                // The end list starts at the first reference not inside a
                // type relationship.
                ks.iter()
                    .position(|k| matches!(k.kind(), "qualified_name" | "feature_chain"))
                    .unwrap_or(ks.len())
            });
        for k in &ks[..first_end] {
            self.decl_part(e, *k);
        }
        self.ends_then_body(e, &ks[first_end..]);
    }

    /// Connector ends followed by the body.
    fn ends_then_body(&mut self, e: ElementRef, ks: &[Node]) {
        let ends: Vec<Node> = ks
            .iter()
            .copied()
            .filter(|k| !BODIES.contains(&k.kind()))
            .collect();
        self.connector_ends(e, &ends);
        for k in ks {
            if BODIES.contains(&k.kind()) {
                self.body(e, *k);
            }
        }
    }

    /// `binding b of x = y;` / `bind x = y;`.
    fn binding(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let first_end = ks
            .iter()
            .position(|k| matches!(k.kind(), "bind" | "of"))
            .map(|i| i + 1)
            .or_else(|| {
                let eq = ks.iter().position(|k| k.kind() == "=")?;
                // The reference before '='.
                ks[..eq]
                    .iter()
                    .rposition(|k| matches!(k.kind(), "qualified_name" | "feature_chain"))
            })
            .unwrap_or(ks.len());
        for k in &ks[..first_end] {
            if k.kind() != "binding" {
                self.decl_part(e, *k);
            }
        }
        self.ends_then_body(e, &ks[first_end..]);
    }

    /// `succession s first a then b;` / `first a then b;`.
    fn succession(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let first_end = ks
            .iter()
            .position(|k| k.kind() == "first")
            .map(|i| i + 1)
            .or_else(|| {
                let then = ks.iter().position(|k| k.kind() == "then")?;
                ks[..then]
                    .iter()
                    .rposition(|k| matches!(k.kind(), "qualified_name" | "feature_chain"))
            })
            .unwrap_or(ks.len());
        for k in &ks[..first_end] {
            self.decl_part(e, *k);
        }
        self.ends_then_body(e, &ks[first_end..]);
    }

    /// `first x;` (initial node) or `first a then b;` (a succession) in an
    /// action body.
    fn first_statement(&mut self, ns: ElementRef, node: Node, prefix: &Prefix) {
        let ks = kids(node);
        if ks.iter().any(|k| k.kind() == "then") {
            let meta = if self.kerml {
                M::Succession
            } else {
                M::SuccessionAsUsage
            };
            let e = self.declaration(node, meta, prefix, BodyKind::Type);
            self.own_member(ns, M::FeatureMembership, e, prefix.visibility);
        } else if let Some(target) = ks
            .iter()
            .find(|k| matches!(k.kind(), "qualified_name" | "feature_chain"))
        {
            // InitialNodeMember: a FeatureMembership whose memberFeature is
            // the named feature (not owned).
            let span = self.span_from(prefix.start.unwrap_or(node.start_byte()), node);
            let m = self.rel(ns, M::FeatureMembership, span);
            self.set_end(m, Role::Target, *target);
        }
    }

    /// Flows and messages: `flow [name] [of T] from a.x to b.y`.
    fn flow(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let mut i = 0;
        let mut mult = None;
        let mut ends_started = false;
        let message = node.kind() == "message_statement";
        while i < ks.len() {
            let k = ks[i];
            match k.kind() {
                "of" => {
                    // Payload feature: `of [name :] T [mult]`.
                    let kind = if self.kerml {
                        M::PayloadFeature
                    } else {
                        M::ReferenceUsage
                    };
                    let p = self.new_el(kind, self.span(k));
                    let mut j = i + 1;
                    while j < ks.len()
                        && !matches!(ks[j].kind(), "from" | "to" | ";")
                        && !BODIES.contains(&ks[j].kind())
                    {
                        let t = ks[j];
                        match t.kind() {
                            "qualified_name" | "feature_chain" => {
                                let r = self.rel(p, M::FeatureTyping, self.span(t));
                                self.set_end(r, Role::Target, t);
                            }
                            _ => {
                                self.decl_part(p, t);
                            }
                        }
                        j += 1;
                    }
                    self.own_member(e, M::FeatureMembership, p, None);
                    i = j;
                    continue;
                }
                "from" | "to" => ends_started = true,
                "multiplicity" if ends_started => mult = Some(k),
                "qualified_name" | "feature_chain" => {
                    ends_started = true;
                    if message {
                        // MessageEvent: an event occurrence usage
                        // referencing the end.
                        let ev = self.new_el(M::EventOccurrenceUsage, self.span(k));
                        let r = self.rel(ev, M::ReferenceSubsetting, self.span(k));
                        self.set_end(r, Role::Target, k);
                        self.own_member(e, M::ParameterMembership, ev, None);
                    } else {
                        self.flow_end(e, k, mult.take());
                    }
                }
                _ => {
                    self.decl_part(e, k);
                }
            }
            i += 1;
        }
    }

    /// `FlowEnd`: `a.b.x` → end with reference subsetting `a.b` and a flow
    /// feature redefining `x`.
    fn flow_end(&mut self, flow: ElementRef, target: Node, mult: Option<Node>) {
        let kind = if self.kerml {
            M::FlowEnd
        } else {
            M::ReferenceUsage
        };
        let end = self.new_el(kind, self.span(target));
        self.el(end).flags.insert(Flags::END);
        if let Some(m) = mult {
            let cross = self.new_el(M::Feature, self.span(m));
            self.owned_multiplicity(cross, m);
            self.own_member(end, M::OwningMembership, cross, None);
        }
        // Split the chain into prefix (reference) and last segment (flow feature).
        let segments: Vec<Node> = if target.kind() == "feature_chain" {
            kids(target)
                .into_iter()
                .filter(|k| matches!(k.kind(), "qualified_name" | "identifier"))
                .collect()
        } else {
            vec![target]
        };
        if segments.len() >= 2 {
            let r = self.rel(end, M::ReferenceSubsetting, self.span(target));
            if segments.len() == 2 {
                let t = Target::Name(self.reference(r, segments[0]));
                self.el(r).ends.push(End {
                    role: Role::Target,
                    target: t,
                });
            } else {
                let chain = self.new_el(M::Feature, self.span(target));
                for s in &segments[..segments.len() - 1] {
                    let c = self.rel(chain, M::FeatureChaining, self.span(*s));
                    let t = Target::Name(self.reference(c, *s));
                    self.el(c).ends.push(End {
                        role: Role::Target,
                        target: t,
                    });
                }
                self.g.attach(r, chain);
                self.el(r).ends.push(End {
                    role: Role::Target,
                    target: Target::Owned(chain),
                });
            }
            let last = *segments.last().unwrap();
            let ff_kind = self.ref_usage_kind();
            let ff = self.new_el(ff_kind, self.span(last));
            let rd = self.rel(ff, M::Redefinition, self.span(last));
            let t = Target::Name(self.reference(rd, last));
            self.el(rd).ends.push(End {
                role: Role::Target,
                target: t,
            });
            self.own_member(end, M::FeatureMembership, ff, None);
        } else {
            // A single name: the end references the feature itself.
            let r = self.rel(end, M::ReferenceSubsetting, self.span(target));
            self.set_end(r, Role::Target, target);
        }
        self.own_member(flow, M::EndFeatureMembership, end, None);
    }
}

/// Metaclass of `<keywords> def`.
fn definition_metaclass(kw: &[&str]) -> Metaclass {
    match kw {
        ["part"] => M::PartDefinition,
        ["action"] => M::ActionDefinition,
        ["port"] => M::PortDefinition,
        ["attribute"] => M::AttributeDefinition,
        ["item"] => M::ItemDefinition,
        ["requirement"] => M::RequirementDefinition,
        ["constraint"] => M::ConstraintDefinition,
        ["view"] => M::ViewDefinition,
        ["viewpoint"] => M::ViewpointDefinition,
        ["rendering"] => M::RenderingDefinition,
        ["concern"] => M::ConcernDefinition,
        ["allocation"] => M::AllocationDefinition,
        ["interface"] => M::InterfaceDefinition,
        ["occurrence"] => M::OccurrenceDefinition,
        ["metadata"] => M::MetadataDefinition,
        ["calc"] => M::CalculationDefinition,
        ["case"] => M::CaseDefinition,
        ["flow"] | ["flow", "connection"] => M::FlowDefinition,
        ["connection"] => M::ConnectionDefinition,
        ["use", "case"] => M::UseCaseDefinition,
        ["analysis"] | ["analysis", "case"] => M::AnalysisCaseDefinition,
        ["verification"] | ["verification", "case"] => M::VerificationCaseDefinition,
        ["individual"] => M::OccurrenceDefinition,
        ["class"] => M::Class,
        ["struct"] => M::Structure,
        ["behavior"] => M::Behavior,
        ["datatype"] => M::DataType,
        ["feature"] => M::Feature,
        ["function"] => M::Function,
        ["predicate"] => M::Predicate,
        ["connector"] => M::Connector,
        ["interaction"] => M::Interaction,
        ["type"] => M::Type,
        ["namespace"] => M::Namespace,
        ["classifier"] => M::Classifier,
        ["metaclass"] => M::Metaclass,
        ["expr"] => M::Expression,
        ["step"] => M::Step,
        ["assoc"] => M::Association,
        ["assoc", "struct"] => M::AssociationStructure,
        _ => M::Definition,
    }
}

/// Metaclass of a SysML keyword usage (`part x`, `attribute y`, ...).
fn usage_metaclass(tokens: &[&str], kerml: bool) -> Metaclass {
    let first = tokens.first().copied().unwrap_or("");
    let second = tokens.get(1).copied().unwrap_or("");
    match first {
        "part" => M::PartUsage,
        "attribute" => M::AttributeUsage,
        "port" => M::PortUsage,
        "item" => M::ItemUsage,
        "occurrence" => M::OccurrenceUsage,
        "calc" => M::CalculationUsage,
        "view" => M::ViewUsage,
        "viewpoint" => M::ViewpointUsage,
        "rendering" => M::RenderingUsage,
        "concern" => M::ConcernUsage,
        "analysis" => M::AnalysisCaseUsage,
        "verification" => M::VerificationCaseUsage,
        "enum" => M::EnumerationUsage,
        "message" => M::FlowUsage,
        "case" => M::CaseUsage,
        "use" => M::UseCaseUsage,
        "classifier" => M::Classifier,
        "metaclass" => M::Metaclass,
        "expr" => {
            if kerml {
                M::Expression
            } else {
                M::CalculationUsage
            }
        }
        "step" => M::Step,
        "snapshot" | "timeslice" => match second {
            "part" => M::PartUsage,
            "item" => M::ItemUsage,
            _ => M::OccurrenceUsage,
        },
        _ => M::Usage,
    }
}

/// Metaclass of a KerML keyword declaration.
fn kerml_metaclass(first: &str, assoc_struct: bool, _negated: bool) -> Metaclass {
    match first {
        "class" => M::Class,
        "struct" => M::Structure,
        "datatype" => M::DataType,
        "type" => M::Type,
        "behavior" => M::Behavior,
        "function" => M::Function,
        "predicate" => M::Predicate,
        "interaction" => M::Interaction,
        "feature" => M::Feature,
        "assoc" if assoc_struct => M::AssociationStructure,
        "assoc" => M::Association,
        "connector" => M::Connector,
        "bool" | "boolean" => M::BooleanExpression,
        "inv" | "invariant" => M::Invariant,
        _ => M::Feature,
    }
}
