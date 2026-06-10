//! Extraction and evaluation of constraint and calc definitions.

use tree_sitter::{Node, Parser};

use crate::model::Span;
use crate::parser::{get_language, node_text};
use crate::sim::expr::*;
use crate::sim::expr_parser::extract_expr;

use serde::Serialize;

/// A constraint definition with its parameters and body expression.
#[derive(Debug, Clone, Serialize)]
pub struct ConstraintModel {
    pub name: String,
    pub params: Vec<Parameter>,
    pub expression: Option<Expr>,
    pub span: Span,
}

/// A calculation definition with its parameters and return expression.
#[derive(Debug, Clone, Serialize)]
pub struct CalcModel {
    pub name: String,
    pub params: Vec<Parameter>,
    pub return_name: Option<String>,
    pub return_type: Option<String>,
    pub return_expr: Option<Expr>,
    pub local_bindings: Vec<(String, Expr)>,
    pub span: Span,
}

/// A parameter declaration.
#[derive(Debug, Clone, Serialize)]
pub struct Parameter {
    pub name: String,
    pub type_ref: Option<String>,
    pub direction: ParamDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ParamDirection {
    In,
    Out,
    InOut,
}

/// Extract all constraint definitions from source.
pub fn extract_constraints(file: &str, source: &str) -> Vec<ConstraintModel> {
    let mut parser = Parser::new();
    parser.set_language(&get_language()).unwrap();
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let source_bytes = source.as_bytes();
    let mut results = Vec::new();
    collect_constraints(tree.root_node(), source_bytes, file, &mut results);
    results
}

fn is_definition_of(node: &Node, source: &[u8], keyword: &str) -> bool {
    if node.kind() == "definition" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if !child.is_named() && node_text(&child, source) == keyword {
                return true;
            }
            if !child.is_named() && node_text(&child, source) == "def" {
                break;
            }
        }
    }
    false
}

#[allow(clippy::only_used_in_recursion)]
fn collect_constraints(node: Node, source: &[u8], file: &str, results: &mut Vec<ConstraintModel>) {
    if node.kind() == "constraint_definition" || is_definition_of(&node, source, "constraint") {
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = node_text(&name_node, source).to_string();
            let mut params = Vec::new();
            let mut expression = None;

            // Find definition_body
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "definition_body" {
                    extract_constraint_body(&child, source, &mut params, &mut expression);
                }
            }

            results.push(ConstraintModel {
                name,
                params,
                expression,
                span: Span::from_node(&node),
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_constraints(child, source, file, results);
    }
}

fn extract_constraint_body(
    body: &Node,
    source: &[u8],
    params: &mut Vec<Parameter>,
    expression: &mut Option<Expr>,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "feature_usage" => {
                if let Some(param) = extract_parameter(&child, source) {
                    params.push(param);
                }
            }
            "expression_statement" => {
                if let Ok(expr) = extract_expr(&child, source) {
                    *expression = Some(expr);
                }
            }
            "result_expression" => {
                if let Ok(expr) = extract_expr(&child, source) {
                    *expression = Some(expr);
                }
            }
            _ => {}
        }
    }
}

/// Extract all calc definitions from source.
pub fn extract_calculations(file: &str, source: &str) -> Vec<CalcModel> {
    let mut parser = Parser::new();
    parser.set_language(&get_language()).unwrap();
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let source_bytes = source.as_bytes();
    let mut results = Vec::new();
    collect_calculations(tree.root_node(), source_bytes, file, &mut results);
    results
}

#[allow(clippy::only_used_in_recursion)]
fn collect_calculations(node: Node, source: &[u8], file: &str, results: &mut Vec<CalcModel>) {
    if node.kind() == "calc_definition" || is_definition_of(&node, source, "calc") {
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = node_text(&name_node, source).to_string();
            let mut params = Vec::new();
            let mut return_name = None;
            let mut return_type = None;
            let mut return_expr = None;
            let mut local_bindings = Vec::new();

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "definition_body" {
                    extract_calc_body(
                        &child,
                        source,
                        &mut params,
                        &mut return_name,
                        &mut return_type,
                        &mut return_expr,
                        &mut local_bindings,
                    );
                }
            }

            results.push(CalcModel {
                name,
                params,
                return_name,
                return_type,
                return_expr,
                local_bindings,
                span: Span::from_node(&node),
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_calculations(child, source, file, results);
    }
}

fn extract_calc_body(
    body: &Node,
    source: &[u8],
    params: &mut Vec<Parameter>,
    return_name: &mut Option<String>,
    return_type: &mut Option<String>,
    return_expr: &mut Option<Expr>,
    local_bindings: &mut Vec<(String, Expr)>,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "feature_usage" => {
                if let Some(param) = extract_parameter(&child, source) {
                    params.push(param);
                }
            }
            "attribute_usage" => {
                // Local attribute bindings: attribute name = expr;
                if let Some(name_node) = child.child_by_field_name("name") {
                    let attr_name = node_text(&name_node, source).to_string();
                    // Look for value_assignment child
                    let mut ac = child.walk();
                    for attr_child in child.children(&mut ac) {
                        if attr_child.kind() == "value_assignment" {
                            let mut vc = attr_child.walk();
                            for val_child in attr_child.children(&mut vc) {
                                if val_child.is_named() {
                                    if let Ok(expr) = extract_expr(&val_child, source) {
                                        local_bindings.push((attr_name.clone(), expr));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "return_statement" => {
                // Extract return name, type, and expression
                if let Some(name_node) = child.child_by_field_name("name") {
                    *return_name = Some(node_text(&name_node, source).to_string());
                }
                // Look for typed_by and value_assignment
                let mut rc = child.walk();
                for ret_child in child.children(&mut rc) {
                    if ret_child.kind() == "typed_by" {
                        if let Some(t) = ret_child.child_by_field_name("type") {
                            *return_type = Some(node_text(&t, source).to_string());
                        }
                    }
                    if ret_child.kind() == "value_assignment" {
                        let mut vc = ret_child.walk();
                        for val_child in ret_child.children(&mut vc) {
                            if val_child.is_named() {
                                if let Ok(expr) = extract_expr(&val_child, source) {
                                    *return_expr = Some(expr);
                                }
                            }
                        }
                    }
                }
            }
            "expression_statement" | "result_expression" => {
                // Bare expression in calc body — use as return expression if none set
                if return_expr.is_none() {
                    if let Ok(expr) = extract_expr(&child, source) {
                        *return_expr = Some(expr);
                    }
                }
            }
            _ => {}
        }
    }
}

fn extract_parameter(node: &Node, source: &[u8]) -> Option<Parameter> {
    let mut direction = ParamDirection::In; // default
    let mut name = None;
    let mut type_ref = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "in" => direction = ParamDirection::In,
            "out" => direction = ParamDirection::Out,
            "inout" => direction = ParamDirection::InOut,
            "typed_by" => {
                if let Some(t) = child.child_by_field_name("type") {
                    type_ref = Some(node_text(&t, source).to_string());
                }
            }
            _ => {
                if child.is_named() && name.is_none() {
                    if let Some(n) = node.child_by_field_name("name") {
                        name = Some(node_text(&n, source).to_string());
                    }
                }
            }
        }
    }

    // Also try the field directly
    if name.is_none() {
        if let Some(n) = node.child_by_field_name("name") {
            name = Some(node_text(&n, source).to_string());
        }
    }

    name.map(|n| Parameter {
        name: n,
        type_ref,
        direction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::eval;

    #[test]
    fn extract_simple_constraint() {
        let source = r#"
            constraint def MassConstraint {
                in massActual : Real;
                in massLimit : Real;
                massActual <= massLimit;
            }
        "#;
        let constraints = extract_constraints("test.sysml", source);
        assert_eq!(constraints.len(), 1);
        assert_eq!(constraints[0].name, "MassConstraint");
        assert_eq!(constraints[0].params.len(), 2);
        assert_eq!(constraints[0].params[0].name, "massActual");
        assert_eq!(constraints[0].params[1].name, "massLimit");
        assert!(constraints[0].expression.is_some());
    }

    #[test]
    fn evaluate_extracted_constraint() {
        let source = r#"
            constraint def MassConstraint {
                in massActual : Real;
                in massLimit : Real;
                massActual <= massLimit;
            }
        "#;
        let constraints = extract_constraints("test.sysml", source);
        let c = &constraints[0];
        let expr = c.expression.as_ref().unwrap();

        let mut env = Env::new();
        env.bind("massActual", Value::Number(1500.0));
        env.bind("massLimit", Value::Number(2000.0));
        assert!(eval::evaluate_constraint(expr, &env).unwrap());

        env.bind("massActual", Value::Number(2500.0));
        assert!(!eval::evaluate_constraint(expr, &env).unwrap());
    }

    #[test]
    fn extract_calc_with_return() {
        let source = r#"
            calc def GoodCalc {
                in x : Real;
                return result : Real;
            }
        "#;
        let calcs = extract_calculations("test.sysml", source);
        assert_eq!(calcs.len(), 1);
        assert_eq!(calcs[0].name, "GoodCalc");
        assert_eq!(calcs[0].params.len(), 1);
        assert_eq!(calcs[0].params[0].name, "x");
        assert!(calcs[0].return_name.is_some());
    }

    #[test]
    fn extract_multiple_constraints() {
        let source = r#"
            constraint def A {
                in x : Real;
                x > 0;
            }
            constraint def B {
                in y : Real;
                y < 100;
            }
        "#;
        let constraints = extract_constraints("test.sysml", source);
        assert_eq!(constraints.len(), 2);
        assert_eq!(constraints[0].name, "A");
        assert_eq!(constraints[1].name, "B");
    }

    #[test]
    fn empty_constraint_no_expr() {
        let source = "constraint def Empty;";
        let constraints = extract_constraints("test.sysml", source);
        assert_eq!(constraints.len(), 1);
        assert!(constraints[0].expression.is_none());
    }
}
