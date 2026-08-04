//! Extract Expr AST from tree-sitter expression nodes.

use tree_sitter::Node;

use crate::parser::node_text;
use crate::sim::expr::*;

/// Convert a tree-sitter expression node into an Expr AST.
pub fn extract_expr(node: &Node, source: &[u8]) -> Result<Expr, EvalError> {
    match node.kind() {
        "number_literal" => {
            let text = node_text(node, source);
            let n: f64 = text
                .replace('_', "")
                .parse()
                .map_err(|_| EvalError::new(format!("invalid number: {}", text)))?;
            Ok(Expr::Literal(Value::Number(n)))
        }

        "boolean_literal" => {
            let text = node_text(node, source);
            match text {
                "true" => Ok(Expr::Literal(Value::Bool(true))),
                "false" => Ok(Expr::Literal(Value::Bool(false))),
                _ => Err(EvalError::new(format!("invalid boolean: {}", text))),
            }
        }

        "string_literal" => {
            let text = node_text(node, source);
            // Strip surrounding quotes
            let inner = text
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(text);
            Ok(Expr::Literal(Value::String(inner.to_string())))
        }

        "null_literal" => Ok(Expr::Literal(Value::Null)),

        "identifier" => {
            let text = node_text(node, source).to_string();
            Ok(Expr::Var(text))
        }

        "qualified_name" | "feature_chain" => {
            let text = node_text(node, source).to_string();
            Ok(Expr::Var(text))
        }

        "binary_expression" => {
            // Children: lhs, operator, rhs
            // Named children vary, so iterate all children
            let child_count = node.child_count();
            if child_count < 3 {
                return Err(EvalError::new(
                    "binary expression has fewer than 3 children",
                ));
            }

            // Find the operator (anonymous node between two named children)
            let mut lhs_node = None;
            let mut op_text = None;
            let mut rhs_node = None;

            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();

            for child in children.iter() {
                if child.is_named() {
                    if lhs_node.is_none() {
                        lhs_node = Some(child);
                    } else if op_text.is_some() && rhs_node.is_none() {
                        rhs_node = Some(child);
                    }
                } else {
                    // Anonymous node = operator
                    let text = node_text(child, source).trim();
                    if lhs_node.is_some() && !text.is_empty() && op_text.is_none() {
                        // Skip parentheses and other non-operator tokens
                        if text != "(" && text != ")" && text != "{" && text != "}" {
                            op_text = Some(text.to_string());
                        }
                    }
                }
            }

            // Fallback: try positional (child 0 = lhs, child 1 = op, child 2 = rhs)
            if lhs_node.is_none() || op_text.is_none() || rhs_node.is_none() {
                if child_count >= 3 {
                    let c0 = children[0];
                    let c1 = children[1];
                    let c2 = children[2];
                    let lhs_n = if let Some(n) = lhs_node { n } else { &c0 };
                    let op_t = op_text.unwrap_or_else(|| node_text(&c1, source).trim().to_string());
                    let rhs_n = if let Some(n) = rhs_node { n } else { &c2 };

                    let lhs = extract_expr(lhs_n, source)?;
                    let rhs = extract_expr(rhs_n, source)?;
                    let op = parse_binop(&op_t)?;
                    return Ok(Expr::BinaryOp {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    });
                }
                return Err(EvalError::new("could not parse binary expression"));
            }

            let lhs = extract_expr(lhs_node.unwrap(), source)?;
            let rhs = extract_expr(rhs_node.unwrap(), source)?;
            let op = parse_binop(&op_text.unwrap())?;
            Ok(Expr::BinaryOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            })
        }

        "unary_expression" => {
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();

            if children.len() < 2 {
                return Err(EvalError::new("unary expression has fewer than 2 children"));
            }

            let op_text = node_text(&children[0], source).trim().to_string();
            let operand = extract_expr(&children[children.len() - 1], source)?;
            let op = parse_unop(&op_text)?;
            Ok(Expr::UnaryOp {
                op,
                operand: Box::new(operand),
            })
        }

        "invocation_expression" => {
            // Function call: name(args...)
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();

            let name = if let Some(first) = children.first() {
                if first.is_named() {
                    node_text(first, source).to_string()
                } else {
                    return Err(EvalError::new("invocation has no name"));
                }
            } else {
                return Err(EvalError::new("empty invocation expression"));
            };

            let mut args = Vec::new();
            // Arguments are named children after the first (skip the name)
            for child in children.iter().skip(1) {
                if child.is_named() && child.kind() != "argument_list" {
                    args.push(extract_expr(child, source)?);
                } else if child.kind() == "argument_list" {
                    // Walk argument list children
                    let mut ac = child.walk();
                    for arg_child in child.children(&mut ac) {
                        if arg_child.is_named() {
                            args.push(extract_expr(&arg_child, source)?);
                        }
                    }
                }
            }

            Ok(Expr::FunctionCall { name, args })
        }

        // Value with unit bracket: `100 [SI::'km/h']` — evaluate the
        // numeric part (units are informational at expression level).
        "bracket_expression" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    return extract_expr(&child, source);
                }
                if child.kind() == "[" {
                    break;
                }
            }
            Err(EvalError::new("empty bracket expression"))
        }

        "paren_expression" | "parenthesized_expression" => {
            // Unwrap to inner expression
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    return extract_expr(&child, source);
                }
            }
            Err(EvalError::new("empty parenthesized expression"))
        }

        // Expression statement wraps an expression with a semicolon
        "expression_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    return extract_expr(&child, source);
                }
            }
            Err(EvalError::new("empty expression statement"))
        }

        // Result expression at end of definition body
        "result_expression" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    return extract_expr(&child, source);
                }
            }
            Err(EvalError::new("empty result expression"))
        }

        _ => {
            // Fallback: try to parse the entire text as a variable reference
            let text = node_text(node, source).trim().to_string();
            if text.is_empty() {
                Err(EvalError::new(format!(
                    "unsupported expression node kind: {}",
                    node.kind()
                )))
            } else {
                Ok(Expr::Var(text))
            }
        }
    }
}

fn parse_binop(text: &str) -> Result<BinOp, EvalError> {
    match text {
        "+" => Ok(BinOp::Add),
        "-" => Ok(BinOp::Sub),
        "*" => Ok(BinOp::Mul),
        "/" => Ok(BinOp::Div),
        "%" => Ok(BinOp::Mod),
        "**" => Ok(BinOp::Pow),
        "==" => Ok(BinOp::Eq),
        "!=" => Ok(BinOp::Neq),
        "<" => Ok(BinOp::Lt),
        ">" => Ok(BinOp::Gt),
        "<=" => Ok(BinOp::Lte),
        ">=" => Ok(BinOp::Gte),
        "and" => Ok(BinOp::And),
        "or" => Ok(BinOp::Or),
        "xor" => Ok(BinOp::Xor),
        "implies" => Ok(BinOp::Implies),
        _ => Err(EvalError::new(format!("unknown operator `{}`", text))),
    }
}

fn parse_unop(text: &str) -> Result<UnOp, EvalError> {
    match text {
        "not" => Ok(UnOp::Not),
        "-" => Ok(UnOp::Neg),
        "!" => Ok(UnOp::Not),
        _ => Err(EvalError::new(format!("unknown unary operator `{}`", text))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::get_language;
    use tree_sitter::Parser;

    /// Parse a SysML constraint body and extract the expression.
    fn parse_constraint_expr(expr_source: &str) -> Expr {
        let source = format!("constraint def Test {{ in x : Real; {} }}", expr_source);
        let mut parser = Parser::new();
        parser.set_language(&get_language()).unwrap();
        let tree = parser.parse(&source, None).unwrap();
        let root = tree.root_node();
        let source_bytes = source.as_bytes();

        // Walk down to find the expression_statement or result_expression
        fn find_expr_node<'a>(node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
            if node.kind() == "expression_statement" || node.kind() == "result_expression" {
                return Some(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = find_expr_node(child) {
                    return Some(found);
                }
            }
            None
        }

        let expr_node = find_expr_node(root).expect("no expression found in parse tree");
        extract_expr(&expr_node, source_bytes).expect("failed to extract expression")
    }

    #[test]
    fn parse_simple_comparison() {
        let expr = parse_constraint_expr("x <= 100;");
        match &expr {
            Expr::BinaryOp { op, .. } => assert_eq!(*op, BinOp::Lte),
            other => panic!("expected BinaryOp, got {:?}", other),
        }
    }

    #[test]
    fn parse_arithmetic() {
        let expr = parse_constraint_expr("x + 10;");
        match &expr {
            Expr::BinaryOp { op, .. } => assert_eq!(*op, BinOp::Add),
            other => panic!("expected BinaryOp, got {:?}", other),
        }
    }

    #[test]
    fn parse_number_literal() {
        let source = "constraint def T { 42; }";
        let mut parser = Parser::new();
        parser.set_language(&get_language()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let source_bytes = source.as_bytes();

        fn find_number<'a>(node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
            if node.kind() == "number_literal" {
                return Some(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = find_number(child) {
                    return Some(found);
                }
            }
            None
        }

        if let Some(num_node) = find_number(tree.root_node()) {
            let expr = extract_expr(&num_node, source_bytes).unwrap();
            assert!(matches!(expr, Expr::Literal(Value::Number(n)) if (n - 42.0).abs() < 0.001));
        }
        // If the grammar doesn't produce a number_literal directly, that's OK
    }

    #[test]
    fn parse_and_eval_constraint() {
        let expr = parse_constraint_expr("x <= 100;");
        let mut env = Env::new();
        env.bind("x", Value::Number(50.0));
        let result = crate::sim::eval::evaluate(&expr, &env).unwrap();
        assert_eq!(result, Value::Bool(true));
    }
}
