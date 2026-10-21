//! Lowering of the expression sublanguage (KerML 8.2.5.8) to the explicit
//! abstract syntax: operator, invocation, feature reference and chain
//! expressions with their argument parameters and result parameters, and
//! literals.
//!
//! Argument features follow the structure of the reference implementation's
//! XMI (a `ParameterMembership` owning an input `Feature` whose
//! `FeatureValue` holds the argument expression; operator arguments are
//! private), so expression elements get the same paths and ids.

use tree_sitter::Node;

use super::decl::BodyKind;
use super::{kids, Lowerer};
use crate::graph::{Direction, ElementRef, End, Flags, Role, Target, Value, Visibility};
use crate::metaclass::Metaclass as M;

/// Operators whose second operand is passed as an expression reference
/// (KerML `ConditionalBinaryOperator`).
const CONDITIONAL_OPS: &[&str] = &["??", "or", "and", "implies"];
const TYPE_TEST_OPS: &[&str] = &["istype", "hastype", "@", "@@"];

impl<'g, 's> Lowerer<'g, 's> {
    /// Lower an expression node into an (unowned) expression element.
    pub(crate) fn expression(&mut self, node: Node) -> ElementRef {
        match node.kind() {
            "identifier" | "qualified_name" => self.feature_reference_expression(node),
            "feature_chain" => self.feature_chain_expression(node),
            "number_literal" => self.number_literal(node),
            "string_literal" => {
                let e = self.new_el(M::LiteralString, self.span(node));
                let v = self.string_value(node);
                self.el(e).value = Value::String(v);
                e
            }
            "boolean_literal" => {
                let e = self.new_el(M::LiteralBoolean, self.span(node));
                let v = self.text(node).trim() == "true";
                self.el(e).value = Value::Bool(v);
                e
            }
            "null_literal" => self.new_el(M::NullExpression, self.span(node)),
            "*" => self.new_el(M::LiteralInfinity, self.span(node)),
            "binary_expression" => self.binary_expression(node),
            "unary_expression" => self.unary_expression(node),
            "paren_expression" => self.paren_expression(node),
            "bracket_expression" => {
                let ks: Vec<Node> = kids(node).into_iter().filter(|k| k.is_named()).collect();
                let e = self.operator_expression("[", node);
                for k in ks.iter().take(2) {
                    self.argument(e, *k, true);
                }
                self.result(e, node);
                e
            }
            "index_expression" => {
                let ks: Vec<Node> = kids(node).into_iter().filter(|k| k.is_named()).collect();
                let e = self.new_el(M::IndexExpression, self.span(node));
                for k in ks.iter().take(2) {
                    self.argument(e, *k, true);
                }
                self.result(e, node);
                e
            }
            "invocation_expression" => self.invocation(node),
            "new_expression" => self.constructor(node),
            "conditional_expression" => {
                let ks: Vec<Node> = kids(node).into_iter().filter(|k| k.is_named()).collect();
                let e = self.operator_expression("if", node);
                if let Some(c) = ks.first() {
                    self.argument(e, *c, true);
                }
                for k in ks.iter().skip(1).take(2) {
                    self.expression_argument(e, *k);
                }
                self.result(e, node);
                e
            }
            "select_expression" => self.select_expression(node),
            "metadata_access_expression" => {
                // `@x`: classification against x with no explicit argument.
                let e = self.operator_expression("@", node);
                if let Some(t) = kids(node).into_iter().find(|k| k.is_named()) {
                    self.type_reference_argument(e, t);
                }
                self.result(e, node);
                e
            }
            "meta_expression" => {
                let ks: Vec<Node> = kids(node).into_iter().filter(|k| k.is_named()).collect();
                let e = self.operator_expression("meta", node);
                if let Some(x) = ks.first() {
                    let f = self.new_el(M::Feature, self.span(*x));
                    self.el(f).direction = Some(Direction::In);
                    let fv = self.rel(f, M::FeatureValue, self.span(*x));
                    let ma = self.new_el(M::MetadataAccessExpression, self.span(*x));
                    let m = self.rel(ma, M::Membership, self.span(*x));
                    self.set_end(m, Role::Target, *x);
                    self.g.attach(fv, ma);
                    let pm = self.own_member(e, M::ParameterMembership, f, None);
                    self.el(pm).visibility = Visibility::Private;
                }
                if let Some(t) = ks.get(1) {
                    self.type_result(e, *t);
                }
                e
            }
            "body_expression" | "definition_body" | "constraint_body" => self.body_expression(node),
            "named_argument" => {
                // Only meaningful inside an argument list.
                let x = kids(node).into_iter().filter(|k| k.is_named()).nth(1);
                match x {
                    Some(x) => self.expression(x),
                    None => self.new_el(M::NullExpression, self.span(node)),
                }
            }
            "result_expression" | "filter_expression" => match node.named_child(0) {
                Some(x) => self.expression(x),
                None => self.new_el(M::NullExpression, self.span(node)),
            },
            _ => {
                // Unknown or erroneous: keep a placeholder expression so the
                // surrounding structure stays intact.
                self.new_el(M::NullExpression, self.span(node))
            }
        }
    }

    fn operator_expression(&mut self, op: &str, node: Node) -> ElementRef {
        let e = self.new_el(M::OperatorExpression, self.span(node));
        self.el(e).value = Value::Operator(op.into());
        e
    }

    /// `ArgumentMember`: an input parameter whose value is `expr`.
    pub(crate) fn argument(&mut self, owner: ElementRef, expr: Node, private: bool) -> ElementRef {
        let x = self.expression(expr);
        self.argument_of(owner, x, expr, private)
    }

    fn argument_of(
        &mut self,
        owner: ElementRef,
        x: ElementRef,
        at: Node,
        private: bool,
    ) -> ElementRef {
        let f = self.new_el(M::Feature, self.span(at));
        self.el(f).direction = Some(Direction::In);
        let fv = self.rel(f, M::FeatureValue, self.span(at));
        self.g.attach(fv, x);
        let pm = self.own_member(owner, M::ParameterMembership, f, None);
        if private {
            self.el(pm).visibility = Visibility::Private;
        }
        f
    }

    /// `ArgumentExpressionMember`: the argument is a reference to the
    /// expression (evaluated lazily by conditional operators).
    fn expression_argument(&mut self, owner: ElementRef, expr: Node) {
        let x = self.expression(expr);
        let fre = self.new_el(M::FeatureReferenceExpression, self.span(expr));
        self.own_member(fre, M::FeatureMembership, x, None);
        self.result(fre, expr);
        self.argument_of(owner, fre, expr, true);
    }

    /// `EmptyResultMember`.
    pub(crate) fn result(&mut self, owner: ElementRef, at: Node) {
        let f = self.new_el(M::Feature, self.span(at));
        self.el(f).direction = Some(Direction::Out);
        self.own_member(owner, M::ReturnParameterMembership, f, None);
    }

    /// `TypeReferenceMember`: a parameter typed by the named type.
    fn type_reference_argument(&mut self, owner: ElementRef, t: Node) {
        let f = self.new_el(M::Feature, self.span(t));
        self.el(f).direction = Some(Direction::In);
        let ty = self.rel(f, M::FeatureTyping, self.span(t));
        self.set_end(ty, Role::Target, t);
        let pm = self.own_member(owner, M::ParameterMembership, f, None);
        self.el(pm).visibility = Visibility::Private;
    }

    /// `TypeResultMember`: the result typed by the named type (casts).
    fn type_result(&mut self, owner: ElementRef, t: Node) {
        let f = self.new_el(M::Feature, self.span(t));
        self.el(f).direction = Some(Direction::Out);
        let ty = self.rel(f, M::FeatureTyping, self.span(t));
        self.set_end(ty, Role::Target, t);
        self.own_member(owner, M::ReturnParameterMembership, f, None);
    }

    /// `FeatureReferenceExpression` for a (qualified) name.
    pub(crate) fn feature_reference_expression(&mut self, node: Node) -> ElementRef {
        if node.kind() == "feature_chain" {
            return self.feature_chain_expression(node);
        }
        let e = self.new_el(M::FeatureReferenceExpression, self.span(node));
        let m = self.rel(e, M::Membership, self.span(node));
        self.set_end(m, Role::Target, node);
        self.result(e, node);
        e
    }

    /// `a.b.c` in an expression: a feature chain expression on the
    /// reference to `a` with the member chain `b.c`.
    fn feature_chain_expression(&mut self, node: Node) -> ElementRef {
        let segments: Vec<Node> = kids(node)
            .into_iter()
            .filter(|k| matches!(k.kind(), "qualified_name" | "identifier"))
            .collect();
        let Some((first, rest)) = segments.split_first() else {
            return self.new_el(M::NullExpression, self.span(node));
        };
        let source = self.feature_reference_expression(*first);
        self.chain_on(source, *first, rest, node)
    }

    /// `FeatureChainExpression` with `source` as its argument and the
    /// member named by `rest`.
    fn chain_on(&mut self, source: ElementRef, at: Node, rest: &[Node], node: Node) -> ElementRef {
        let e = self.new_el(M::FeatureChainExpression, self.span(node));
        let arg = self.argument_of(e, source, at, true);
        // The argument's nested feature (target of the implied source::target
        // redefinition), present in the explicit model with an empty name.
        let nested = self.new_el(M::Feature, self.span(at));
        self.el(nested).name = Some("".into());
        self.own_member(arg, M::FeatureMembership, nested, None);
        if rest.len() == 1 {
            let m = self.rel(e, M::Membership, self.span(rest[0]));
            self.set_end(m, Role::Target, rest[0]);
        } else if !rest.is_empty() {
            let chain = self.new_el(M::Feature, self.span(node));
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
        self.result(e, node);
        e
    }

    fn number_literal(&mut self, node: Node) -> ElementRef {
        let text = self.text(node).trim();
        let is_real = !text.starts_with("0x")
            && !text.starts_with("0X")
            && (text.contains('.') || text.contains('e') || text.contains('E'));
        if is_real {
            let e = self.new_el(M::LiteralRational, self.span(node));
            self.el(e).value = Value::Rational(text.into());
            e
        } else {
            let e = self.new_el(M::LiteralInteger, self.span(node));
            let value =
                if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                    u128::from_str_radix(hex, 16)
                        .map(|v| v.to_string())
                        .unwrap_or_else(|_| text.to_owned())
                } else {
                    text.to_owned()
                };
            self.el(e).value = Value::Integer(value.into());
            e
        }
    }

    fn binary_expression(&mut self, node: Node) -> ElementRef {
        let ks = kids(node);
        let Some(op) = ks.iter().find(|k| !k.is_named()).map(|k| k.kind()) else {
            return self.new_el(M::NullExpression, self.span(node));
        };
        let operands: Vec<Node> = ks.iter().copied().filter(|k| k.is_named()).collect();
        match op {
            "." => {
                // `expr.name`: a feature chain on an arbitrary expression.
                let lhs = operands[0];
                let rhs: Vec<Node> = operands[1..]
                    .iter()
                    .flat_map(|r| {
                        if r.kind() == "feature_chain" {
                            kids(*r)
                                .into_iter()
                                .filter(|k| matches!(k.kind(), "qualified_name" | "identifier"))
                                .collect()
                        } else {
                            vec![*r]
                        }
                    })
                    .collect();
                let source = self.expression(lhs);
                return self.chain_on(source, lhs, &rhs, node);
            }
            "::" => {
                // `a::b` that the grammar split: a reference to the whole name.
                return self.feature_reference_expression(node);
            }
            "->" => return self.arrow_invocation(node, &ks),
            _ => {}
        }
        let e = self.operator_expression(op, node);
        if TYPE_TEST_OPS.contains(&op) {
            self.argument(e, operands[0], true);
            if let Some(t) = operands.get(1) {
                self.type_reference_argument(e, *t);
            }
            self.result(e, node);
        } else if op == "as" {
            self.argument(e, operands[0], true);
            if let Some(t) = operands.get(1) {
                self.type_result(e, *t);
            }
        } else if CONDITIONAL_OPS.contains(&op) {
            self.argument(e, operands[0], true);
            if let Some(b) = operands.get(1) {
                self.expression_argument(e, *b);
            }
            self.result(e, node);
        } else {
            for o in operands.iter().take(2) {
                self.argument(e, *o, true);
            }
            self.result(e, node);
        }
        e
    }

    /// `x->f(args)` / `x->f {body}` / `x->f 'op'`.
    fn arrow_invocation(&mut self, node: Node, ks: &[Node]) -> ElementRef {
        let e = self.new_el(M::InvocationExpression, self.span(node));
        let arrow = ks.iter().position(|k| k.kind() == "->").unwrap_or(0);
        if let Some(x) = ks[..arrow].iter().find(|k| k.is_named()) {
            self.argument(e, *x, true);
        }
        let after = &ks[arrow + 1..];
        let mut function_done = false;
        let mut i = 0;
        while i < after.len() {
            let k = after[i];
            match k.kind() {
                "identifier" | "qualified_name" if !function_done => {
                    function_done = true;
                    let m = self.rel(e, M::Membership, self.span(k));
                    self.set_end(m, Role::Target, k);
                }
                "{" => {
                    // Body argument: everything up to the matching '}'.
                    let close = after
                        .iter()
                        .rposition(|c| c.kind() == "}")
                        .unwrap_or(after.len());
                    let body_nodes: Vec<Node> = after[i + 1..close].to_vec();
                    let body = self.expression_body_from(&body_nodes, node);
                    let fre = self.new_el(M::FeatureReferenceExpression, self.span(node));
                    self.own_member(fre, M::FeatureMembership, body, None);
                    self.result(fre, node);
                    self.argument_of(e, fre, node, false);
                    i = close + 1;
                    continue;
                }
                "(" | ")" | "," => {}
                "quoted_identifier" | "feature_chain" => {
                    // Function reference argument (`->reduce '+'`).
                    let fre = self.new_el(M::FeatureReferenceExpression, self.span(k));
                    let f = self.new_el(M::Expression, self.span(k));
                    let ty = self.rel(f, M::FeatureTyping, self.span(k));
                    self.set_end(ty, Role::Target, k);
                    self.own_member(fre, M::FeatureMembership, f, None);
                    self.result(fre, k);
                    self.argument_of(e, fre, k, false);
                }
                "identifier" | "qualified_name"
                    if function_done && after.iter().all(|c| c.kind() != "(") =>
                {
                    // `->reduce RealFunctions::'+'`
                    let fre = self.new_el(M::FeatureReferenceExpression, self.span(k));
                    let f = self.new_el(M::Expression, self.span(k));
                    let ty = self.rel(f, M::FeatureTyping, self.span(k));
                    self.set_end(ty, Role::Target, k);
                    self.own_member(fre, M::FeatureMembership, f, None);
                    self.result(fre, k);
                    self.argument_of(e, fre, k, false);
                }
                "named_argument" => self.named_argument(e, k),
                _ if k.is_named() => {
                    self.argument(e, k, false);
                }
                _ => {}
            }
            i += 1;
        }
        self.result(e, node);
        e
    }

    fn unary_expression(&mut self, node: Node) -> ElementRef {
        let ks = kids(node);
        let op = ks.first().map(|k| k.kind()).unwrap_or("");
        let operand = ks.iter().copied().find(|k| k.is_named());
        let e = self.operator_expression(op, node);
        if let Some(x) = operand {
            if op == "all" {
                self.type_reference_argument(e, x);
            } else {
                self.argument(e, x, true);
            }
        }
        self.result(e, node);
        e
    }

    fn paren_expression(&mut self, node: Node) -> ElementRef {
        let ks = kids(node);
        let operands: Vec<Node> = ks.iter().copied().filter(|k| k.is_named()).collect();
        if ks.iter().any(|k| k.kind() == "..") && operands.len() == 2 {
            return self.range_expression(operands[0], operands[1]);
        }
        match operands.len() {
            0 => self.new_el(M::NullExpression, self.span(node)),
            1 => self.expression(operands[0]),
            _ => self.sequence(&operands, node),
        }
    }

    /// `(a, b, c)` → `','(a, ','(b, c))`.
    fn sequence(&mut self, operands: &[Node], node: Node) -> ElementRef {
        let e = self.operator_expression(",", node);
        self.argument(e, operands[0], true);
        if operands.len() == 2 {
            self.argument(e, operands[1], true);
        } else {
            let rest = self.sequence(&operands[1..], node);
            self.argument_of(e, rest, operands[1], true);
        }
        self.result(e, node);
        e
    }

    /// `a..b`.
    pub(crate) fn range_expression(&mut self, a: Node, b: Node) -> ElementRef {
        let e = self.new_el(M::OperatorExpression, self.span(a));
        self.el(e).value = Value::Operator("..".into());
        self.argument(e, a, true);
        self.argument(e, b, true);
        self.result(e, a);
        e
    }

    fn invocation(&mut self, node: Node) -> ElementRef {
        let ks = kids(node);
        let e = self.new_el(M::InvocationExpression, self.span(node));
        let mut function_done = false;
        for k in ks {
            match k.kind() {
                "identifier" | "qualified_name" if !function_done => {
                    function_done = true;
                    let m = self.rel(e, M::Membership, self.span(k));
                    self.set_end(m, Role::Target, k);
                }
                "named_argument" => self.named_argument(e, k),
                "(" | ")" | "," => {}
                _ if k.is_named() => {
                    self.argument(e, k, false);
                }
                _ => {}
            }
        }
        self.result(e, node);
        e
    }

    /// `name = value` in an argument list: a feature redefining the named
    /// parameter with the value.
    fn named_argument(&mut self, owner: ElementRef, node: Node) {
        let ks: Vec<Node> = kids(node).into_iter().filter(|k| k.is_named()).collect();
        let f = self.new_el(M::Feature, self.span(node));
        self.el(f).direction = Some(Direction::In);
        if let Some(name) = ks.first() {
            let r = self.rel(f, M::Redefinition, self.span(*name));
            self.set_end(r, Role::Target, *name);
        }
        if let Some(v) = ks.get(1) {
            let fv = self.rel(f, M::FeatureValue, self.span(*v));
            let x = self.expression(*v);
            self.g.attach(fv, x);
        }
        self.own_member(owner, M::ParameterMembership, f, None);
    }

    /// `new T(args)`.
    fn constructor(&mut self, node: Node) -> ElementRef {
        let ks = kids(node);
        let e = self.new_el(M::ConstructorExpression, self.span(node));
        let result = self.new_el(M::Feature, self.span(node));
        self.el(result).direction = Some(Direction::Out);
        let mut type_done = false;
        for k in ks {
            match k.kind() {
                "identifier" | "qualified_name" if !type_done => {
                    type_done = true;
                    let m = self.rel(e, M::Membership, self.span(k));
                    self.set_end(m, Role::Target, k);
                }
                "named_argument" => self.named_argument(result, k),
                "(" | ")" | "," | "new" => {}
                _ if k.is_named() => {
                    self.argument(result, k, false);
                }
                _ => {}
            }
        }
        self.own_member(e, M::ReturnParameterMembership, result, None);
        e
    }

    /// `x.?{...}` (select) / `x.{...}` (collect).
    fn select_expression(&mut self, node: Node) -> ElementRef {
        let ks = kids(node);
        let select = ks.iter().any(|k| k.kind() == ".?");
        let e = self.new_el(
            if select {
                M::SelectExpression
            } else {
                M::CollectExpression
            },
            self.span(node),
        );
        if let Some(x) = ks.first().filter(|k| k.is_named()) {
            self.argument(e, *x, true);
        }
        let open = ks
            .iter()
            .position(|k| matches!(k.kind(), "{" | ".{"))
            .unwrap_or(0);
        let close = ks.iter().rposition(|k| k.kind() == "}").unwrap_or(ks.len());
        let body_nodes: Vec<Node> = ks[(open + 1).min(close)..close].to_vec();
        let body = self.expression_body_from(&body_nodes, node);
        let fre = self.new_el(M::FeatureReferenceExpression, self.span(node));
        self.own_member(fre, M::FeatureMembership, body, None);
        self.result(fre, node);
        self.argument_of(e, fre, node, false);
        self.result(e, node);
        e
    }

    /// `{ in x; expr }` as an expression: a feature reference to an owned
    /// body expression.
    pub(crate) fn body_expression(&mut self, node: Node) -> ElementRef {
        let ks = kids(node);
        let inner: Vec<Node> = ks
            .iter()
            .copied()
            .filter(|k| !matches!(k.kind(), "{" | "}"))
            .collect();
        let body = self.expression_body_from(&inner, node);
        let fre = self.new_el(M::FeatureReferenceExpression, self.span(node));
        self.own_member(fre, M::FeatureMembership, body, None);
        self.result(fre, node);
        fre
    }

    /// `ExpressionBody`: an `Expression` whose members are the given body
    /// elements; a trailing bare expression is its result expression.
    fn expression_body_from(&mut self, items: &[Node], span_node: Node) -> ElementRef {
        let body = self.new_el(M::Expression, self.span(span_node));
        let mut prefix = super::Prefix::default();
        for (i, k) in items.iter().enumerate() {
            let last_expr = i + 1 == items.len() || items[i + 1..].iter().all(|n| !n.is_named());
            match k.kind() {
                "modifier" => {
                    prefix.modifiers.push(match self.text(*k).trim() {
                        "in" => "in",
                        "out" => "out",
                        "inout" => "inout",
                        "ref" => "ref",
                        "abstract" => "abstract",
                        _ => "",
                    });
                    prefix.start.get_or_insert(k.start_byte());
                }
                "visibility" => {}
                _ if k.is_named() && is_expression_kind(k.kind()) && last_expr => {
                    let x = self.expression(*k);
                    self.own_member(body, M::ResultExpressionMembership, x, None);
                }
                "expression_statement" | "result_expression" if last_expr => {
                    if let Some(x) = k.named_child(0) {
                        let x = self.expression(x);
                        self.own_member(body, M::ResultExpressionMembership, x, None);
                    }
                }
                _ if k.is_named() => {
                    // Body declarations (`in x;`): lower them into the body.
                    let tmp_prefix = std::mem::take(&mut prefix);
                    self.lower_body_item(body, *k, tmp_prefix);
                }
                _ => {}
            }
        }
        body
    }

    fn lower_body_item(&mut self, body: ElementRef, node: Node, prefix: super::Prefix) {
        // Reuse the generic member lowering via a one-element body walk.
        self.member_pub(body, node, prefix, BodyKind::Type);
    }

    /// A filter condition: `@T` (metaclassification of the filtered
    /// element) or a boolean expression.
    pub(crate) fn filter_expression(&mut self, node: Node) -> ElementRef {
        let ks = kids(node);
        if ks.first().map(|k| k.kind()) == Some("@") {
            let e = self.operator_expression("@@", node);
            if let Some(t) = ks.iter().copied().find(|k| k.is_named()) {
                self.type_reference_argument(e, t);
            }
            self.result(e, node);
            return e;
        }
        match ks.iter().copied().find(|k| k.is_named()) {
            Some(x) => self.expression(x),
            None => self.new_el(M::NullExpression, self.span(node)),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn mark_private(&mut self, membership: ElementRef) {
        let el = self.el(membership);
        el.visibility = Visibility::Private;
        el.flags.insert(Flags::NONE);
    }
}

/// Whether a CST node kind is an expression.
pub(crate) fn is_expression_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "qualified_name"
            | "feature_chain"
            | "number_literal"
            | "string_literal"
            | "boolean_literal"
            | "null_literal"
            | "binary_expression"
            | "unary_expression"
            | "paren_expression"
            | "bracket_expression"
            | "index_expression"
            | "invocation_expression"
            | "new_expression"
            | "meta_expression"
            | "conditional_expression"
            | "select_expression"
            | "metadata_access_expression"
            | "body_expression"
    )
}
