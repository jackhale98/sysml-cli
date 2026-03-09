/// Extract action flow models from tree-sitter parse trees.

use tree_sitter::{Node, Parser};

use crate::model::Span;
use crate::parser::{get_language, node_text};
use crate::sim::action_flow::*;
use crate::sim::expr_parser::extract_expr;

/// Extract all action definitions from source.
pub fn extract_actions(file: &str, source: &str) -> Vec<ActionModel> {
    let mut parser = Parser::new();
    parser.set_language(&get_language()).unwrap();
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let source_bytes = source.as_bytes();
    let mut results = Vec::new();
    collect_action_nodes(tree.root_node(), source_bytes, file, &mut results);
    results
}

fn collect_action_nodes(
    node: Node,
    source: &[u8],
    _file: &str,
    results: &mut Vec<ActionModel>,
) {
    match node.kind() {
        "action_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(&name_node, source).to_string();
                let mut steps = Vec::new();

                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "definition_body" {
                        extract_action_body(&child, source, &mut steps);
                    }
                }

                results.push(ActionModel {
                    name,
                    steps,
                    span: Span::from_node(&node),
                });
            }
        }
        "action_usage" => {
            // Also extract action usages that have bodies (inline action definitions)
            if let Some(name_node) = node.child_by_field_name("name") {
                let has_body = node
                    .children(&mut node.walk())
                    .any(|c| c.kind() == "definition_body");
                if has_body {
                    let name = node_text(&name_node, source).to_string();
                    let mut steps = Vec::new();

                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind() == "definition_body" {
                            extract_action_body(&child, source, &mut steps);
                        }
                    }

                    // Only add if it has meaningful steps
                    if !steps.is_empty() {
                        results.push(ActionModel {
                            name,
                            steps,
                            span: Span::from_node(&node),
                        });
                    }
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_action_nodes(child, source, _file, results);
    }
}

fn extract_action_body(body: &Node, source: &[u8], steps: &mut Vec<ActionStep>) {
    let children: Vec<Node> = body.children(&mut body.walk()).collect();
    let mut i = 0;
    while i < children.len() {
        let child = &children[i];
        if let Some(step) = extract_step(child, source) {
            // Check if this is an if_action followed by else_action — pair them
            if matches!(step, ActionStep::IfAction { .. }) {
                if let Some(next) = children.get(i + 1) {
                    if next.kind() == "else_action" {
                        let else_step = extract_else_action(next, source);
                        if let ActionStep::IfAction {
                            condition,
                            then_step,
                            span,
                            ..
                        } = step
                        {
                            steps.push(ActionStep::IfAction {
                                condition,
                                then_step,
                                else_step: else_step.map(Box::new),
                                span,
                            });
                            i += 2;
                            continue;
                        }
                    }
                }
            }
            steps.push(step);
        }
        i += 1;
    }
}

fn extract_step(node: &Node, source: &[u8]) -> Option<ActionStep> {
    match node.kind() {
        "action_usage" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(&n, source).to_string())?;
            Some(ActionStep::Perform {
                name,
                span: Span::from_node(node),
            })
        }
        "perform_statement" => {
            let mut name = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(
                    child.kind(),
                    "identifier" | "qualified_name" | "feature_chain"
                ) {
                    let text = node_text(&child, source).to_string();
                    if text != "perform" && text != "action" {
                        name = Some(text);
                        break;
                    }
                }
            }
            name.map(|n| ActionStep::Perform {
                name: n,
                span: Span::from_node(node),
            })
        }
        "then_succession" => extract_then_succession(node, source),
        "succession_statement" => extract_succession_statement(node, source),
        "fork_node" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(&n, source).to_string());
            let mut branches = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "definition_body" {
                    extract_action_body(&child, source, &mut branches);
                }
            }
            Some(ActionStep::Fork {
                name,
                branches,
                span: Span::from_node(node),
            })
        }
        "join_node" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(&n, source).to_string());
            Some(ActionStep::Join {
                name,
                span: Span::from_node(node),
            })
        }
        "decide_node" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(&n, source).to_string());
            Some(ActionStep::Decide {
                name,
                branches: Vec::new(),
                span: Span::from_node(node),
            })
        }
        "merge_node" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(&n, source).to_string());
            Some(ActionStep::Merge {
                name,
                span: Span::from_node(node),
            })
        }
        "if_action" => extract_if_action(node, source),
        "assign_action" => extract_assign_action(node, source),
        "send_action" => extract_send_action(node, source),
        "while_action" => extract_while_action(node, source),
        "for_action" => extract_for_action(node, source),
        "accept_clause" => extract_accept_clause(node, source),
        "terminate_statement" => extract_terminate_statement(node, source),
        "flow_usage" => extract_flow_usage(node, source),
        _ => None,
    }
}

/// Extract a `then_succession` node — e.g., `then action X;`, `then merge m;`,
/// `then accept S;`, `then send ...`, `then decide;`, `then terminate;`
fn extract_then_succession(node: &Node, source: &[u8]) -> Option<ActionStep> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    // Check for specific child node types first
    for child in &children {
        match child.kind() {
            // Nested action_usage inside then_succession
            "action_usage" => {
                if let Some(step) = extract_step(child, source) {
                    return Some(step);
                }
            }
            // accept clause: `then accept S;`
            "accept_clause" => {
                return extract_accept_clause(child, source);
            }
            // definition_body inside then (inline action body)
            "definition_body" => {
                let mut steps = Vec::new();
                extract_action_body(child, source, &mut steps);
                if !steps.is_empty() {
                    return Some(if steps.len() == 1 {
                        steps.into_iter().next().unwrap()
                    } else {
                        ActionStep::Sequence {
                            steps,
                            span: Span::from_node(node),
                        }
                    });
                }
            }
            // terminate_statement inside then
            "terminate_statement" => {
                return extract_terminate_statement(child, source);
            }
            _ => {}
        }
    }

    // Check for keyword-based patterns
    let has_merge = children.iter().any(|c| c.kind() == "merge");
    let has_send = children.iter().any(|c| c.kind() == "send");
    let has_decide = children.iter().any(|c| c.kind() == "decide");
    let has_terminate = children.iter().any(|c| c.kind() == "terminate");

    if has_merge {
        // `then merge m;` — get the name after merge
        let name = children
            .iter()
            .filter(|c| matches!(c.kind(), "identifier" | "qualified_name"))
            .find_map(|c| {
                let text = node_text(c, source).to_string();
                if text != "then" && text != "merge" {
                    Some(text)
                } else {
                    None
                }
            });
        return Some(ActionStep::Merge {
            name,
            span: Span::from_node(node),
        });
    }

    if has_send {
        // `then send new S() to b;` — extract send details
        let mut payload = None;
        let mut to = None;
        let mut after_to = false;
        for child in &children {
            match child.kind() {
                "to" => after_to = true,
                "new_expression" => {
                    // Extract the type name from `new S()`
                    for nc in child.children(&mut child.walk()) {
                        if nc.kind() == "qualified_name" {
                            payload = Some(node_text(&nc, source).to_string());
                            break;
                        }
                    }
                }
                "identifier" | "qualified_name" | "feature_chain" => {
                    let text = node_text(child, source).to_string();
                    if text == "then" || text == "send" {
                        continue;
                    }
                    if after_to {
                        to = Some(text);
                        after_to = false;
                    } else if payload.is_none() {
                        payload = Some(text);
                    }
                }
                _ => {}
            }
        }
        return Some(ActionStep::Send {
            payload,
            via: None,
            to,
            span: Span::from_node(node),
        });
    }

    if has_decide {
        return Some(ActionStep::Decide {
            name: None,
            branches: Vec::new(),
            span: Span::from_node(node),
        });
    }

    if has_terminate {
        let target = children
            .iter()
            .filter(|c| matches!(c.kind(), "identifier" | "qualified_name"))
            .find_map(|c| {
                let text = node_text(c, source).to_string();
                if text != "then" && text != "terminate" {
                    Some(text)
                } else {
                    None
                }
            });
        return Some(ActionStep::Terminate {
            target,
            span: Span::from_node(node),
        });
    }

    // Fallback: look for a plain identifier reference (e.g., `then actionName;`)
    let name = children
        .iter()
        .filter(|c| matches!(c.kind(), "identifier" | "qualified_name" | "feature_chain"))
        .find_map(|c| {
            let text = node_text(c, source).to_string();
            if text != "then" && text != "action" {
                Some(text)
            } else {
                None
            }
        });
    name.map(|n| ActionStep::Perform {
        name: n,
        span: Span::from_node(node),
    })
}

/// Extract a `succession_statement` — e.g., `first A then B;` or `first start;`
fn extract_succession_statement(node: &Node, source: &[u8]) -> Option<ActionStep> {
    let mut refs = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "identifier" | "qualified_name" | "feature_chain"
        ) {
            let text = node_text(&child, source).to_string();
            if text != "first" && text != "then" {
                refs.push(text);
            }
        }
    }
    match refs.len() {
        0 => None,
        1 => Some(ActionStep::Perform {
            name: refs.into_iter().next().unwrap(),
            span: Span::from_node(node),
        }),
        _ => Some(ActionStep::Sequence {
            steps: refs
                .into_iter()
                .map(|name| ActionStep::Perform {
                    name,
                    span: Span::from_node(node),
                })
                .collect(),
            span: Span::from_node(node),
        }),
    }
}

/// Extract an `accept_clause` — `accept S`, `accept when condition`, `accept at time`
fn extract_accept_clause(node: &Node, source: &[u8]) -> Option<ActionStep> {
    let mut signal = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "qualified_name" => {
                let text = node_text(&child, source).to_string();
                if text != "accept" {
                    signal = Some(text);
                    break;
                }
            }
            "feature_chain" => {
                signal = Some(node_text(&child, source).to_string());
                break;
            }
            _ => {}
        }
    }
    Some(ActionStep::Accept {
        signal,
        span: Span::from_node(node),
    })
}

/// Extract a `terminate_statement` — `terminate;` or `terminate name;`
fn extract_terminate_statement(node: &Node, source: &[u8]) -> Option<ActionStep> {
    let mut target = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "identifier" | "qualified_name") {
            let text = node_text(&child, source).to_string();
            if text != "terminate" {
                target = Some(text);
                break;
            }
        }
    }
    Some(ActionStep::Terminate {
        target,
        span: Span::from_node(node),
    })
}

/// Extract a `flow_usage` — `flow source to target;`
fn extract_flow_usage(node: &Node, source: &[u8]) -> Option<ActionStep> {
    let mut from = None;
    let mut to = None;
    let mut after_to = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "to" => after_to = true,
            "identifier" | "qualified_name" | "feature_chain" => {
                let text = node_text(&child, source).to_string();
                if text == "flow" {
                    continue;
                }
                if after_to {
                    to = Some(text);
                } else if from.is_none() {
                    from = Some(text);
                }
            }
            _ => {}
        }
    }
    Some(ActionStep::Send {
        payload: from,
        via: None,
        to,
        span: Span::from_node(node),
    })
}

/// Extract the else branch from an `else_action` node.
fn extract_else_action(node: &Node, source: &[u8]) -> Option<ActionStep> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "qualified_name" | "feature_chain" => {
                let text = node_text(&child, source).to_string();
                if text == "else" {
                    continue;
                }
                if text == "done" {
                    return Some(ActionStep::Done {
                        span: Span::from_node(&child),
                    });
                }
                return Some(ActionStep::Perform {
                    name: text,
                    span: Span::from_node(&child),
                });
            }
            "if_action" => {
                return extract_if_action(&child, source);
            }
            _ => {}
        }
    }
    None
}

fn extract_if_action(node: &Node, source: &[u8]) -> Option<ActionStep> {
    let mut condition = None;
    let mut then_ref = None;
    let mut else_ref = None;
    let mut saw_then = false;
    let mut saw_else = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "then" => saw_then = true,
            "else" => saw_else = true,
            "if_action" => {
                // Nested if-else chain
                if saw_else {
                    else_ref = extract_if_action(&child, source);
                }
            }
            "boolean_literal" => {
                if condition.is_none() && !saw_then {
                    let text = node_text(&child, source).trim().to_string();
                    condition = Some(if text == "true" {
                        crate::sim::expr::Expr::Literal(crate::sim::expr::Value::Bool(true))
                    } else {
                        crate::sim::expr::Expr::Literal(crate::sim::expr::Value::Bool(false))
                    });
                }
            }
            "identifier" | "qualified_name" | "feature_chain" => {
                let text = node_text(&child, source).to_string();
                if text == "if" || text == "then" || text == "else" {
                    continue;
                }
                if saw_else {
                    if text == "done" {
                        else_ref = Some(ActionStep::Done {
                            span: Span::from_node(&child),
                        });
                    } else {
                        else_ref = Some(ActionStep::Perform {
                            name: text,
                            span: Span::from_node(&child),
                        });
                    }
                } else if saw_then {
                    then_ref = Some(ActionStep::Perform {
                        name: text,
                        span: Span::from_node(&child),
                    });
                } else if condition.is_none() {
                    condition = extract_expr(&child, source).ok();
                }
            }
            _ => {
                if condition.is_none() && child.is_named() && !saw_then {
                    condition = extract_expr(&child, source).ok();
                }
            }
        }
    }

    let cond = condition?;
    let then_step = then_ref?;

    Some(ActionStep::IfAction {
        condition: cond,
        then_step: Box::new(then_step),
        else_step: else_ref.map(Box::new),
        span: Span::from_node(node),
    })
}

fn extract_assign_action(node: &Node, source: &[u8]) -> Option<ActionStep> {
    let mut target = None;
    let mut value = None;
    let mut saw_assign_op = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "qualified_name" | "feature_chain" => {
                let text = node_text(&child, source).to_string();
                if text == "assign" {
                    continue;
                }
                if saw_assign_op {
                    value = extract_expr(&child, source).ok();
                } else {
                    target = Some(text);
                }
            }
            _ => {
                let text = node_text(&child, source).trim().to_string();
                if text == ":=" {
                    saw_assign_op = true;
                } else if saw_assign_op && child.is_named() && value.is_none() {
                    value = extract_expr(&child, source).ok();
                }
            }
        }
    }

    let tgt = target?;
    let val = value?;

    Some(ActionStep::Assign {
        target: tgt,
        value: val,
        span: Span::from_node(node),
    })
}

fn extract_send_action(node: &Node, source: &[u8]) -> Option<ActionStep> {
    let mut payload = None;
    let mut via = None;
    let mut to = None;
    let mut after_via = false;
    let mut after_to = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "via" => after_via = true,
            "to" => after_to = true,
            "identifier" | "qualified_name" | "feature_chain" => {
                let text = node_text(&child, source).to_string();
                if text == "send" {
                    continue;
                }
                if after_to {
                    to = Some(text);
                    after_to = false;
                } else if after_via {
                    via = Some(text);
                    after_via = false;
                } else if payload.is_none() {
                    payload = Some(text);
                }
            }
            _ => {}
        }
    }

    Some(ActionStep::Send {
        payload,
        via,
        to,
        span: Span::from_node(node),
    })
}

fn extract_while_action(node: &Node, source: &[u8]) -> Option<ActionStep> {
    let mut condition = None;
    let mut body_ref = None;
    let mut saw_do = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "do" => saw_do = true,
            "identifier" | "qualified_name" | "feature_chain" => {
                let text = node_text(&child, source).to_string();
                if text == "while" || text == "do" {
                    continue;
                }
                if saw_do {
                    body_ref = Some(ActionStep::Perform {
                        name: text,
                        span: Span::from_node(&child),
                    });
                } else if condition.is_none() {
                    condition = extract_expr(&child, source).ok();
                }
            }
            _ => {
                if condition.is_none() && child.is_named() && !saw_do {
                    condition = extract_expr(&child, source).ok();
                }
            }
        }
    }

    let cond = condition?;
    let body = body_ref?;

    Some(ActionStep::WhileLoop {
        condition: cond,
        body: Box::new(body),
        span: Span::from_node(node),
    })
}

fn extract_for_action(node: &Node, source: &[u8]) -> Option<ActionStep> {
    let mut variable = None;
    let mut collection = None;
    let mut body_ref = None;
    let mut saw_in = false;
    let mut saw_do = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "in" => saw_in = true,
            "do" => saw_do = true,
            "identifier" | "qualified_name" | "feature_chain" => {
                let text = node_text(&child, source).to_string();
                if text == "for" || text == "in" || text == "do" {
                    continue;
                }
                if saw_do {
                    body_ref = Some(ActionStep::Perform {
                        name: text,
                        span: Span::from_node(&child),
                    });
                } else if saw_in {
                    collection = Some(text);
                } else if variable.is_none() {
                    variable = Some(text);
                }
            }
            _ => {}
        }
    }

    let var = variable?;
    let coll = collection?;
    let body = body_ref.unwrap_or(ActionStep::Done {
        span: Span::from_node(node),
    });

    Some(ActionStep::ForLoop {
        variable: var,
        collection: coll,
        body: Box::new(body),
        span: Span::from_node(node),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_simple_action() {
        let source = r#"
            action def ProcessOrder {
                action validate;
                then action ship;
                then action notify;
            }
        "#;
        let actions = extract_actions("test.sysml", source);
        assert_eq!(actions.len(), 1);
        let a = &actions[0];
        assert_eq!(a.name, "ProcessOrder");
        assert!(a.steps.len() >= 1, "expected steps, got {}", a.steps.len());
    }

    #[test]
    fn extract_action_with_succession() {
        let source = r#"
            action def Pipeline {
                action step1;
                action step2;
                action step3;
                first step1 then step2;
                first step2 then step3;
            }
        "#;
        let actions = extract_actions("test.sysml", source);
        assert_eq!(actions.len(), 1);
        let a = &actions[0];
        // Should have action usages + succession statements
        assert!(a.steps.len() >= 3);
    }

    #[test]
    fn extract_action_usage_with_body() {
        let source = r#"
            action a1 {
                action step1;
                then action step2;
                then action step3;
            }
        "#;
        let actions = extract_actions("test.sysml", source);
        assert!(!actions.is_empty(), "should extract action_usage with body");
        let a = actions.iter().find(|a| a.name == "a1").unwrap();
        assert!(a.steps.len() >= 2);
    }

    #[test]
    fn extract_first_start() {
        let source = r#"
            action def MyAction {
                first start;
                then action doWork;
            }
        "#;
        let actions = extract_actions("test.sysml", source);
        assert_eq!(actions.len(), 1);
        let a = &actions[0];
        assert!(a.steps.len() >= 2, "expected >= 2, got {}", a.steps.len());
        // first start should be a Perform("start")
        assert!(
            matches!(&a.steps[0], ActionStep::Perform { name, .. } if name == "start"),
            "expected Perform(start), got {:?}",
            a.steps[0]
        );
    }

    #[test]
    fn extract_then_merge() {
        let source = r#"
            action def WithMerge {
                first start;
                then merge m;
                then action doWork;
            }
        "#;
        let actions = extract_actions("test.sysml", source);
        let a = &actions[0];
        let has_merge = a.steps.iter().any(|s| matches!(s, ActionStep::Merge { .. }));
        assert!(has_merge, "expected Merge step, got {:?}", a.steps);
    }

    #[test]
    fn extract_then_accept() {
        let source = r#"
            action def WithAccept {
                first start;
                then accept S;
            }
        "#;
        let actions = extract_actions("test.sysml", source);
        let a = &actions[0];
        let has_accept = a
            .steps
            .iter()
            .any(|s| matches!(s, ActionStep::Accept { .. }));
        assert!(has_accept, "expected Accept step, got {:?}", a.steps);
    }

    #[test]
    fn extract_then_terminate() {
        let source = r#"
            action def WithTerminate {
                first start;
                then terminate;
            }
        "#;
        let actions = extract_actions("test.sysml", source);
        let a = &actions[0];
        let has_terminate = a
            .steps
            .iter()
            .any(|s| matches!(s, ActionStep::Terminate { .. }));
        assert!(
            has_terminate,
            "expected Terminate step, got {:?}",
            a.steps
        );
    }

    #[test]
    fn no_actions_in_part_file() {
        let source = "part def Vehicle;";
        let actions = extract_actions("test.sysml", source);
        assert!(actions.is_empty());
    }
}
