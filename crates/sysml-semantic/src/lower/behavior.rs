//! Lowering of SysML action and state notation (SysML 8.2.2.17-8.2.2.18):
//! action nodes, successions, transitions and state subactions.

use tree_sitter::Node;

use super::decl::BodyKind;
use super::{kids, Lowerer, Prefix};
use crate::graph::{ElementRef, End, Flags, Role, Target, Value};
use crate::metaclass::Metaclass as M;
use crate::metaclass::Metaclass;

const BODIES: &[&str] = &[
    "definition_body",
    "state_body",
    "requirement_body",
    "constraint_body",
];

fn is_ref(k: Node) -> bool {
    matches!(k.kind(), "qualified_name" | "feature_chain")
}

fn is_expr(k: Node) -> bool {
    k.is_named()
        && !matches!(
            k.kind(),
            "definition_body"
                | "state_body"
                | "multiplicity"
                | "typed_by"
                | "typed_by_keyword"
                | "specialization"
                | "redefinition"
                | "keyword_type_relationship"
                | "value_assignment"
                | "short_name"
                | "accept_clause"
                | "binding"
        )
}

impl<'g, 's> Lowerer<'g, 's> {
    fn empty_usage(&mut self, span_node: Node) -> ElementRef {
        let kind = self.ref_usage_kind();
        self.new_el(kind, self.span(span_node))
    }

    /// `EmptyParameterMember`.
    fn empty_parameter(&mut self, owner: ElementRef, span_node: Node) {
        let u = self.empty_usage(span_node);
        self.own_member(owner, M::ParameterMembership, u, None);
    }

    /// `NodeParameterMember`: a parameter bound to an expression.
    fn node_parameter(&mut self, owner: ElementRef, expr: Node) {
        let u = self.empty_usage(expr);
        let fv = self.rel(u, M::FeatureValue, self.span(expr));
        let x = self.expression(expr);
        self.g.attach(fv, x);
        self.own_member(owner, M::ParameterMembership, u, None);
    }

    /// `ExpressionParameterMember`.
    fn expression_parameter(&mut self, owner: ElementRef, expr: Node) {
        let x = self.expression(expr);
        self.own_member(owner, M::ParameterMembership, x, None);
    }

    /// `ActionBodyParameterMember`: an action usage holding a body.
    fn action_body_parameter(&mut self, owner: ElementRef, body: Node) {
        let a = self.new_el(M::ActionUsage, self.span(body));
        self.body_as(a, body, BodyKind::Type);
        self.own_member(owner, M::ParameterMembership, a, None);
    }

    /// `FeatureChainMember`: a membership referencing a feature (or an
    /// owned feature chain).
    fn feature_chain_member(&mut self, owner: ElementRef, target: Node) {
        if target.kind() == "feature_chain" {
            let chain = self.feature_chain(target);
            self.own_member(owner, M::OwningMembership, chain, None);
        } else {
            let m = self.rel(owner, M::Membership, self.span(target));
            self.set_end(m, Role::Target, target);
        }
    }

    /// `TransitionSuccessionMember`: a succession from an empty source end
    /// to the target.
    fn transition_succession(&mut self, owner: ElementRef, target: Node) {
        let s = self.new_el(M::Succession, self.span(target));
        let empty = self.empty_usage(target);
        self.own_member(s, M::EndFeatureMembership, empty, None);
        self.connector_end(s, target, None, None);
        self.own_member(owner, M::OwningMembership, s, None);
    }

    /// The payload parameter and its options for `accept ...`.
    /// `parts` are the nodes after the `accept` keyword.
    fn accept_parameters(&mut self, a: ElementRef, parts: &[Node]) {
        let mut i = 0;
        // Trigger forms: `when expr`, `at expr`, `after expr`.
        if let Some(first) = parts.first() {
            if matches!(first.kind(), "when" | "at" | "after") {
                let kind: &'static str = match first.kind() {
                    "when" => "when",
                    "at" => "at",
                    _ => "after",
                };
                let p = self.empty_usage(*first);
                if let Some(x) = parts.get(1) {
                    let fv = self.rel(p, M::FeatureValue, self.span(*x));
                    let t = self.new_el(M::TriggerInvocationExpression, self.span(*x));
                    self.el(t).value = Value::Kind(kind);
                    let arg = self.expression(*x);
                    if kind == "when" {
                        // ArgumentExpressionMember: a reference to the expression.
                        let f = self.new_el(M::Feature, self.span(*x));
                        let afv = self.rel(f, M::FeatureValue, self.span(*x));
                        let fre = self.new_el(M::FeatureReferenceExpression, self.span(*x));
                        self.own_member(fre, M::FeatureMembership, arg, None);
                        self.g.attach(afv, fre);
                        self.own_member(t, M::ParameterMembership, f, None);
                    } else {
                        let f = self.new_el(M::Feature, self.span(*x));
                        let afv = self.rel(f, M::FeatureValue, self.span(*x));
                        self.g.attach(afv, arg);
                        self.own_member(t, M::ParameterMembership, f, None);
                    }
                    self.g.attach(fv, t);
                }
                self.own_member(a, M::ParameterMembership, p, None);
                return;
            }
        }
        // `accept [name] [: T] [via port]` / `accept T`.
        let kind = self.ref_usage_kind();
        let p = self.new_el(
            kind,
            parts.first().map(|n| self.span(*n)).unwrap_or_default(),
        );
        let has_typing = parts
            .iter()
            .take_while(|k| !matches!(k.kind(), "via" | "after" | "then"))
            .any(|k| !is_ref(*k) && k.is_named());
        let mut seen_name = false;
        while i < parts.len() {
            let k = parts[i];
            match k.kind() {
                "via" | "then" | "after" => break,
                _ if is_ref(k) && !seen_name => {
                    seen_name = true;
                    if has_typing && k.kind() == "qualified_name" {
                        self.set_name(p, k);
                    } else {
                        // `accept T`: the payload is typed by T.
                        let r = self.rel(p, M::FeatureTyping, self.span(k));
                        self.set_end(r, Role::Target, k);
                    }
                }
                _ => {
                    self.decl_part(p, k);
                }
            }
            i += 1;
        }
        self.own_member(a, M::ParameterMembership, p, None);
        // `after expr` on a named payload (rare) and `via port`.
        while i < parts.len() {
            let k = parts[i];
            if k.kind() == "via" {
                if let Some(x) = parts.get(i + 1) {
                    self.node_parameter(a, *x);
                }
                i += 2;
                continue;
            }
            i += 1;
        }
    }

    /// Action nodes written with their keyword (`action a accept ...`,
    /// `send`, `accept`, `assign`, `if`, `while`, `loop`, `for`, `terminate`).
    pub(crate) fn action_node(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let kind = self.kind_of(e);
        // Declaration part: everything before the node keyword.
        let kw_pos = ks.iter().position(|k| {
            matches!(
                k.kind(),
                "send" | "accept" | "assign" | "if" | "while" | "loop" | "for" | "terminate"
            )
        });
        let (decl, rest) = match kw_pos {
            Some(p) => (&ks[..p], &ks[p + 1..]),
            None => (&ks[..], &ks[ks.len()..]),
        };
        let mut body = None;
        for k in decl {
            if BODIES.contains(&k.kind()) {
                body = Some(*k);
                continue;
            }
            if k.kind() == "action" {
                continue;
            }
            self.decl_part(e, *k);
        }
        match kind {
            M::SendActionUsage => {
                // send [payload] [via sender] [to receiver]
                let mut payload = None;
                let mut via = None;
                let mut to = None;
                let mut state = 0;
                for k in rest {
                    match k.kind() {
                        "via" => state = 1,
                        "to" => state = 2,
                        _ if BODIES.contains(&k.kind()) => body = Some(*k),
                        _ if is_expr(*k) => match state {
                            0 => payload = Some(*k),
                            1 => via = Some(*k),
                            _ => to = Some(*k),
                        },
                        _ => {}
                    }
                }
                if let Some(p) = payload {
                    self.node_parameter(e, p);
                }
                match (via, to) {
                    (Some(v), t) => {
                        self.node_parameter(e, v);
                        if let Some(t) = t {
                            self.node_parameter(e, t);
                        }
                    }
                    (None, Some(t)) => {
                        self.empty_parameter(e, t);
                        self.node_parameter(e, t);
                    }
                    (None, None) => {}
                }
            }
            M::AcceptActionUsage => {
                let parts: Vec<Node> = rest
                    .iter()
                    .copied()
                    .filter(|k| !BODIES.contains(&k.kind()) && k.kind() != ";")
                    .collect();
                body = rest
                    .iter()
                    .copied()
                    .find(|k| BODIES.contains(&k.kind()))
                    .or(body);
                self.accept_parameters(e, &parts);
            }
            M::AssignmentActionUsage => {
                let target = rest.iter().copied().find(|k| is_ref(*k));
                let value = rest
                    .iter()
                    .skip_while(|k| k.kind() != ":=")
                    .skip(1)
                    .copied()
                    .find(|k| is_expr(*k));
                body = rest
                    .iter()
                    .copied()
                    .find(|k| BODIES.contains(&k.kind()))
                    .or(body);
                if let Some(t) = target {
                    self.assignment_target(e, t);
                }
                if let Some(v) = value {
                    self.node_parameter(e, v);
                }
            }
            M::TerminateActionUsage => {
                if let Some(t) = rest.iter().copied().find(|k| is_expr(*k)) {
                    self.node_parameter(e, t);
                }
                body = rest
                    .iter()
                    .copied()
                    .find(|k| BODIES.contains(&k.kind()))
                    .or(body);
            }
            M::IfActionUsage => {
                let mut iter = rest.iter().copied().peekable();
                if let Some(cond) = iter.next() {
                    self.expression_parameter(e, cond);
                }
                let mut in_else = false;
                for k in iter {
                    match k.kind() {
                        "else" => in_else = true,
                        "definition_body" => self.action_body_parameter(e, k),
                        "if_action" if in_else => {
                            let nested = self.declaration(
                                k,
                                M::IfActionUsage,
                                &Prefix::default(),
                                BodyKind::Type,
                            );
                            self.own_member(e, M::ParameterMembership, nested, None);
                        }
                        _ => {}
                    }
                }
                return;
            }
            M::WhileLoopActionUsage => {
                let is_loop = ks.iter().any(|k| k.kind() == "loop");
                let mut until = false;
                let mut seen_body = false;
                if is_loop {
                    self.empty_parameter(e, node);
                }
                let mut cond_done = is_loop;
                for k in rest {
                    match k.kind() {
                        "until" => until = true,
                        "definition_body" => {
                            seen_body = true;
                            self.action_body_parameter(e, *k);
                        }
                        _ if is_expr(*k) && !cond_done => {
                            cond_done = true;
                            self.expression_parameter(e, *k);
                        }
                        _ if is_expr(*k) && until => {
                            self.expression_parameter(e, *k);
                        }
                        _ => {}
                    }
                }
                let _ = seen_body;
                return;
            }
            M::ForLoopActionUsage => {
                // for v [: T] in expr [.. expr] body
                let kind = self.ref_usage_kind();
                let var = self.new_el(kind, self.span(node));
                let mut i = 0;
                let mut seen_in = false;
                let mut range: Vec<Node> = Vec::new();
                while i < rest.len() {
                    let k = rest[i];
                    match k.kind() {
                        "identifier" if !seen_in => self.set_name(var, k),
                        ":" => {}
                        _ if is_ref(k) && !seen_in => {
                            let t = self.rel(var, M::FeatureTyping, self.span(k));
                            self.set_end(t, Role::Target, k);
                        }
                        "in" => seen_in = true,
                        ".." => {}
                        "definition_body" => {}
                        _ if seen_in && is_expr(k) => range.push(k),
                        _ => {}
                    }
                    i += 1;
                }
                self.own_member(e, M::FeatureMembership, var, None);
                if let Some(first) = range.first() {
                    if range.len() == 2 {
                        // `a..b`: an operator expression on the range.
                        let x = self.range_expression(range[0], range[1]);
                        let u = self.empty_usage(*first);
                        let fv = self.rel(u, M::FeatureValue, self.span(*first));
                        self.g.attach(fv, x);
                        self.own_member(e, M::ParameterMembership, u, None);
                    } else {
                        self.node_parameter(e, *first);
                    }
                }
                if let Some(b) = rest.iter().copied().find(|k| k.kind() == "definition_body") {
                    self.action_body_parameter(e, b);
                }
                return;
            }
            _ => {
                for k in rest {
                    self.decl_part(e, *k);
                }
            }
        }
        if let Some(b) = body {
            self.body_as(e, b, BodyKind::Type);
        }
    }

    /// `assign a.b.x := v`: the target parameter (with the `a.b` binding)
    /// and the assigned feature.
    fn assignment_target(&mut self, e: ElementRef, target: Node) {
        let segments: Vec<Node> = if target.kind() == "feature_chain" {
            kids(target)
                .into_iter()
                .filter(|k| matches!(k.kind(), "qualified_name" | "identifier"))
                .collect()
        } else {
            vec![target]
        };
        let param = self.empty_usage(target);
        if segments.len() >= 2 {
            let fv = self.rel(param, M::FeatureValue, self.span(segments[0]));
            let x = self.feature_reference_expression(segments[0]);
            self.g.attach(fv, x);
        }
        self.own_member(e, M::ParameterMembership, param, None);
        let rest = if segments.len() >= 2 {
            &segments[1..]
        } else {
            &segments[..]
        };
        if rest.len() == 1 {
            let m = self.rel(e, M::Membership, self.span(rest[0]));
            self.set_end(m, Role::Target, rest[0]);
        } else {
            let chain = self.new_el(M::Feature, self.span(target));
            for s in rest {
                let c = self.rel(chain, M::FeatureChaining, self.span(*s));
                let t = Target::Name(self.reference(c, *s));
                self.el(c).ends.push(End {
                    role: Role::Target,
                    target: t,
                });
            }
            self.own_member(e, M::OwningMembership, chain, None);
        }
    }

    /// `then ...` in an action body.
    pub(crate) fn then_succession(
        &mut self,
        ns: ElementRef,
        node: Node,
        prefix: Prefix,
        _kind: BodyKind,
    ) {
        let ks = kids(node);
        // Which form? A keyword after `then` declares a new member.
        let second = ks.get(1).map(|k| k.kind()).unwrap_or("");
        let declares = matches!(
            second,
            "action"
                | "state"
                | "fork"
                | "join"
                | "merge"
                | "decide"
                | "event"
                | "terminate"
                | "message"
                | "timeslice"
                | "snapshot"
                | "use"
                | "verification"
                | "analysis"
                | "visibility"
        ) || ks.iter().any(|k| {
            matches!(
                k.kind(),
                "send" | "accept_clause" | "while" | "if" | "assign" | "include" | "perform" | "of"
            )
        }) || ks.iter().any(|k| BODIES.contains(&k.kind()));
        if !declares {
            // TargetSuccession: `then x;`
            if let Some(target) = ks.iter().copied().find(|k| is_ref(*k)) {
                let meta = if self.kerml {
                    M::Succession
                } else {
                    M::SuccessionAsUsage
                };
                let s = self.new_el(meta, self.span(node));
                let empty = self.empty_usage(node);
                self.own_member(s, M::EndFeatureMembership, empty, None);
                self.connector_end(s, target, None, None);
                self.own_member(ns, M::FeatureMembership, s, prefix.visibility);
            }
            return;
        }
        // SourceSuccessionMember, then the declared member.
        let meta = if self.kerml {
            M::Succession
        } else {
            M::SuccessionAsUsage
        };
        let s = self.new_el(meta, self.span(ks[0]));
        let empty = self.empty_usage(ks[0]);
        self.own_member(s, M::EndFeatureMembership, empty, None);
        self.own_member(ns, M::FeatureMembership, s, None);
        // Determine the member's metaclass from the keyword after `then`.
        let rest_kinds: Vec<&str> = ks.iter().map(|k| k.kind()).collect();
        let meta = if rest_kinds.contains(&"send") {
            M::SendActionUsage
        } else if rest_kinds.contains(&"accept_clause") {
            M::AcceptActionUsage
        } else if rest_kinds.contains(&"assign") {
            M::AssignmentActionUsage
        } else if rest_kinds.contains(&"while") {
            M::WhileLoopActionUsage
        } else if rest_kinds.contains(&"if") {
            M::IfActionUsage
        } else if rest_kinds.contains(&"include") {
            M::IncludeUseCaseUsage
        } else if rest_kinds.contains(&"perform") {
            M::PerformActionUsage
        } else {
            match second {
                "action" => M::ActionUsage,
                "state" => M::StateUsage,
                "fork" => M::ForkNode,
                "join" => M::JoinNode,
                "merge" => M::MergeNode,
                "decide" => M::DecisionNode,
                "event" => M::EventOccurrenceUsage,
                "terminate" => M::TerminateActionUsage,
                "message" => M::FlowUsage,
                "timeslice" | "snapshot" => M::OccurrenceUsage,
                "use" => M::UseCaseUsage,
                "verification" => M::VerificationCaseUsage,
                "analysis" => M::AnalysisCaseUsage,
                _ => M::ActionUsage,
            }
        };
        let span = self.span(node);
        let e = self.new_el(meta, span);
        self.el(e).flags.insert(Flags::NONE);
        // The (optional) name is the first reference after the keyword.
        let mut named = false;
        let content: Vec<Node> = ks[1..].to_vec();
        match meta {
            M::SendActionUsage
            | M::AssignmentActionUsage
            | M::WhileLoopActionUsage
            | M::IfActionUsage
            | M::TerminateActionUsage => {
                // Reuse the action node lowering on a synthetic view: the
                // children after `then`.
                self.action_node_from(e, &content);
            }
            M::AcceptActionUsage => {
                for k in &content {
                    match k.kind() {
                        "accept_clause" => {
                            let parts: Vec<Node> = kids(*k).into_iter().skip(1).collect();
                            self.accept_parameters(e, &parts);
                        }
                        _ if is_ref(*k) && !named => {
                            named = true;
                            self.set_name(e, *k);
                        }
                        "definition_body" => self.body_as(e, *k, BodyKind::Type),
                        _ => {}
                    }
                }
            }
            M::PerformActionUsage | M::IncludeUseCaseUsage => {
                let declared = content
                    .iter()
                    .any(|k| matches!(k.kind(), "action" | "case"));
                let mut first_ref = true;
                for k in &content {
                    match k.kind() {
                        _ if is_ref(*k) && first_ref => {
                            first_ref = false;
                            if declared && k.kind() == "qualified_name" {
                                self.set_name(e, *k);
                            } else {
                                let r = self.rel(e, M::ReferenceSubsetting, self.span(*k));
                                self.set_end(r, Role::Target, *k);
                            }
                        }
                        _ => {
                            self.decl_part(e, *k);
                        }
                    }
                }
            }
            _ => {
                for k in &content {
                    match k.kind() {
                        _ if is_ref(*k) && !named && k.kind() == "qualified_name" => {
                            named = true;
                            self.set_name(e, *k);
                        }
                        _ => {
                            self.decl_part(e, *k);
                        }
                    }
                }
            }
        }
        let membership = if prefix.has("variant") {
            M::VariantMembership
        } else {
            M::FeatureMembership
        };
        self.own_member(ns, membership, e, prefix.visibility);
    }

    /// Lower an action node whose keyword and operands are `content`.
    fn action_node_from(&mut self, e: ElementRef, content: &[Node]) {
        let kind = self.kind_of(e);
        match kind {
            M::SendActionUsage => {
                let mut payload = None;
                let mut via = None;
                let mut to = None;
                let mut state = 0;
                for k in content {
                    match k.kind() {
                        "send" => state = 0,
                        "via" => state = 1,
                        "to" => state = 2,
                        "definition_body" => self.body_as(e, *k, BodyKind::Type),
                        _ if is_expr(*k) => match state {
                            0 => payload = Some(*k),
                            1 => via = Some(*k),
                            _ => to = Some(*k),
                        },
                        _ => {}
                    }
                }
                if let Some(p) = payload {
                    self.node_parameter(e, p);
                }
                match (via, to) {
                    (Some(v), t) => {
                        self.node_parameter(e, v);
                        if let Some(t) = t {
                            self.node_parameter(e, t);
                        }
                    }
                    (None, Some(t)) => {
                        self.empty_parameter(e, t);
                        self.node_parameter(e, t);
                    }
                    _ => {}
                }
            }
            M::AssignmentActionUsage => {
                let target = content.iter().copied().find(|k| is_ref(*k));
                let value = content
                    .iter()
                    .skip_while(|k| k.kind() != ":=")
                    .skip(1)
                    .copied()
                    .find(|k| is_expr(*k));
                if let Some(t) = target {
                    self.assignment_target(e, t);
                }
                if let Some(v) = value {
                    self.node_parameter(e, v);
                }
            }
            M::TerminateActionUsage => {
                if let Some(t) = content.iter().copied().find(|k| is_expr(*k)) {
                    self.node_parameter(e, t);
                }
            }
            M::IfActionUsage => {
                let mut cond_done = false;
                let mut in_else = false;
                for k in content {
                    match k.kind() {
                        "else" => in_else = true,
                        "definition_body" => self.action_body_parameter(e, *k),
                        "if_action" if in_else => {
                            let nested = self.declaration(
                                *k,
                                M::IfActionUsage,
                                &Prefix::default(),
                                BodyKind::Type,
                            );
                            self.own_member(e, M::ParameterMembership, nested, None);
                        }
                        _ if is_expr(*k) && !cond_done => {
                            cond_done = true;
                            self.expression_parameter(e, *k);
                        }
                        _ => {}
                    }
                }
            }
            M::WhileLoopActionUsage => {
                let mut cond_done = false;
                let mut until = false;
                for k in content {
                    match k.kind() {
                        "until" => until = true,
                        "definition_body" => self.action_body_parameter(e, *k),
                        _ if is_expr(*k) && (!cond_done || until) => {
                            cond_done = true;
                            self.expression_parameter(e, *k);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// `entry ...;` / `do ...;` / `exit ...;` in a state body.
    pub(crate) fn state_subaction(&mut self, ns: ElementRef, node: Node, prefix: &Prefix) {
        let ks = kids(node);
        let kind: &'static str = match node.kind() {
            "entry_action" => "entry",
            "do_action" => "do",
            _ => "exit",
        };
        let rest: Vec<Node> = ks.iter().skip(1).copied().collect();
        let has = |t: &str| rest.iter().any(|k| k.kind() == t);
        let span = self.span(node);
        let meta = if has("send") {
            M::SendActionUsage
        } else if has("assign") {
            M::AssignmentActionUsage
        } else if has("accept") {
            M::AcceptActionUsage
        } else if rest
            .iter()
            .all(|k| matches!(k.kind(), ";" | "then") || is_ref(*k))
            && !rest.iter().any(|k| is_ref(*k))
        {
            // EmptyActionUsage
            M::ActionUsage
        } else {
            // StatePerformActionUsage
            M::PerformActionUsage
        };
        let e = self.new_el(meta, span);
        self.prefix_metadata_members_pub(e, prefix, node);
        let declared = has("action");
        let mut then_target = None;
        let mut seen_then = false;
        match meta {
            M::SendActionUsage | M::AssignmentActionUsage => {
                let content: Vec<Node> = rest
                    .iter()
                    .copied()
                    .filter(|k| k.kind() != "action")
                    .collect();
                self.action_node_from(e, &content);
            }
            M::PerformActionUsage => {
                let mut first_ref = true;
                for k in &rest {
                    match k.kind() {
                        "then" => seen_then = true,
                        _ if is_ref(*k) && seen_then => then_target = Some(*k),
                        _ if is_ref(*k) && first_ref => {
                            first_ref = false;
                            if declared && k.kind() == "qualified_name" {
                                self.set_name(e, *k);
                            } else {
                                let r = self.rel(e, M::ReferenceSubsetting, self.span(*k));
                                self.set_end(r, Role::Target, *k);
                            }
                        }
                        "action" => {}
                        _ => {
                            self.decl_part(e, *k);
                        }
                    }
                }
            }
            _ => {
                for k in &rest {
                    match k.kind() {
                        "then" => seen_then = true,
                        _ if is_ref(*k) && seen_then => then_target = Some(*k),
                        _ => {}
                    }
                }
            }
        }
        let m = self.own_member(ns, M::StateSubactionMembership, e, prefix.visibility);
        self.el(m).value = Value::Kind(kind);
        // `entry; then s1;`: EntryTransitionMember.
        if let Some(target) = then_target {
            let t = self.new_el(M::TransitionUsage, self.span(target));
            self.empty_parameter(t, target);
            self.transition_succession(t, target);
            self.own_member(ns, M::FeatureMembership, t, None);
        }
    }

    pub(crate) fn prefix_metadata_members_pub(
        &mut self,
        e: ElementRef,
        prefix: &Prefix,
        node: Node,
    ) {
        for tag in prefix
            .metadata
            .iter()
            .copied()
            .chain(kids(node).into_iter().filter(|k| k.kind() == "hash_tag"))
        {
            let kind = self.metadata_kind();
            let mu = self.new_el(kind, self.span(tag));
            if let Some(name) = kids(tag).into_iter().find(|k| k.is_named()) {
                let t = self.rel(mu, M::FeatureTyping, self.span(name));
                self.set_end(t, Role::Target, name);
            }
            self.own_member(e, M::OwningMembership, mu, None);
        }
    }

    /// `transition [name] first S [accept ...] [if g] [do e] then T`.
    pub(crate) fn transition_parts(&mut self, e: ElementRef, node: Node) {
        let ks = kids(node);
        let mut i = 1; // skip `transition`
                       // Declaration (name and specializations) up to `first`.
        while i < ks.len()
            && ks[i].kind() != "first"
            && !matches!(ks[i].kind(), "accept_clause" | "if" | "do" | "then")
        {
            self.decl_part(e, ks[i]);
            i += 1;
        }
        let mut source = None;
        if i < ks.len() && ks[i].kind() == "first" {
            source = ks.get(i + 1).copied();
            i += 2;
        }
        if let Some(s) = source {
            self.feature_chain_member(e, s);
        }
        self.transition_tail(e, &ks[i..], node);
    }

    /// The part of a transition after its source: trigger, guard, effect,
    /// `then` target and body.
    fn transition_tail(&mut self, e: ElementRef, parts: &[Node], span_node: Node) {
        self.empty_parameter(e, span_node);
        let mut i = 0;
        let mut target = None;
        let mut body = None;
        while i < parts.len() {
            let k = parts[i];
            match k.kind() {
                "accept_clause" => {
                    self.empty_parameter(e, k);
                    let a = self.new_el(M::AcceptActionUsage, self.span(k));
                    let accept_parts: Vec<Node> = kids(k).into_iter().skip(1).collect();
                    self.accept_parameters(a, &accept_parts);
                    let m = self.own_member(e, M::TransitionFeatureMembership, a, None);
                    self.el(m).value = Value::Kind("trigger");
                }
                "accept" => {
                    // Inline accept (accept_action): payload up to `then`.
                    self.empty_parameter(e, k);
                    let a = self.new_el(M::AcceptActionUsage, self.span(k));
                    let end = parts[i + 1..]
                        .iter()
                        .position(|p| matches!(p.kind(), "then" | "if" | "do"))
                        .map(|p| i + 1 + p)
                        .unwrap_or(parts.len());
                    let accept_parts: Vec<Node> = parts[i + 1..end].to_vec();
                    self.accept_parameters(a, &accept_parts);
                    let m = self.own_member(e, M::TransitionFeatureMembership, a, None);
                    self.el(m).value = Value::Kind("trigger");
                    i = end;
                    continue;
                }
                "if" => {
                    if let Some(g) = parts.get(i + 1) {
                        let x = self.expression(*g);
                        let m = self.own_member(e, M::TransitionFeatureMembership, x, None);
                        self.el(m).value = Value::Kind("guard");
                    }
                    i += 2;
                    continue;
                }
                "do" => {
                    // EffectBehaviorMember.
                    let end = parts[i + 1..]
                        .iter()
                        .position(|p| p.kind() == "then")
                        .map(|p| i + 1 + p)
                        .unwrap_or(parts.len());
                    let eff_parts: Vec<Node> = parts[i + 1..end].to_vec();
                    let eff = self.effect_behavior(&eff_parts, k);
                    let m = self.own_member(e, M::TransitionFeatureMembership, eff, None);
                    self.el(m).value = Value::Kind("effect");
                    i = end;
                    continue;
                }
                "then" => {
                    target = parts.get(i + 1).copied();
                    i += 2;
                    continue;
                }
                _ if BODIES.contains(&k.kind()) => body = Some(k),
                _ => {}
            }
            i += 1;
        }
        if let Some(t) = target {
            self.transition_succession(e, t);
        }
        if let Some(b) = body {
            self.body_as(e, b, BodyKind::Type);
        }
    }

    /// The effect of a transition: `do send ...`, `do action a : A`,
    /// `do x` (perform).
    fn effect_behavior(&mut self, parts: &[Node], span_node: Node) -> ElementRef {
        let kinds: Vec<&str> = parts.iter().map(|k| k.kind()).collect();
        if kinds.contains(&"send") {
            let e = self.new_el(M::SendActionUsage, self.span(span_node));
            self.action_node_from(e, parts);
            return e;
        }
        let declared = kinds.contains(&"action");
        let e = self.new_el(M::PerformActionUsage, self.span(span_node));
        let mut first_ref = true;
        for k in parts {
            match k.kind() {
                _ if is_ref(*k) && first_ref => {
                    first_ref = false;
                    if declared && k.kind() == "qualified_name" {
                        self.set_name(e, *k);
                    } else {
                        let r = self.rel(e, M::ReferenceSubsetting, self.span(*k));
                        self.set_end(r, Role::Target, *k);
                    }
                }
                "action" => {}
                _ => {
                    self.decl_part(e, *k);
                }
            }
        }
        e
    }

    /// A target transition in a state body or a guarded/default succession
    /// in an action body: `accept E then s;`, `if g then s;`, `else s;`.
    pub(crate) fn transition(&mut self, node: Node, prefix: &Prefix) -> ElementRef {
        let span = self.span_from(prefix.start.unwrap_or(node.start_byte()), node);
        let t = self.new_el(M::TransitionUsage, span);
        let ks = kids(node);
        match node.kind() {
            "else_action" => {
                if let Some(target) = ks.iter().copied().find(|k| is_ref(*k)) {
                    self.transition_succession(t, target);
                }
            }
            "if_action" => {
                // if g then a [else b];
                let cond = ks.get(1).copied();
                if let Some(c) = cond {
                    let x = self.expression(c);
                    let m = self.own_member(t, M::TransitionFeatureMembership, x, None);
                    self.el(m).value = Value::Kind("guard");
                }
                let target = ks
                    .iter()
                    .skip_while(|k| k.kind() != "then")
                    .copied()
                    .find(|k| is_ref(*k));
                if let Some(target) = target {
                    self.transition_succession(t, target);
                }
            }
            _ => {
                // inline_transition: accept_clause [if g] [do ...] then t;
                // accept_action: accept ... then t;
                self.transition_tail(t, &ks, node);
            }
        }
        t
    }

    /// A transition usage member: the if_action `then` form.
    pub(crate) fn guarded_succession(&mut self, ns: ElementRef, node: Node, prefix: &Prefix) {
        let t = self.transition(node, prefix);
        self.own_member(ns, M::FeatureMembership, t, prefix.visibility);
    }

    /// The membership kind for successions in bodies.
    #[allow(dead_code)]
    pub(crate) fn succession_kind(&self) -> Metaclass {
        if self.kerml {
            M::Succession
        } else {
            M::SuccessionAsUsage
        }
    }
}
