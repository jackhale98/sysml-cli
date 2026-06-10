//! Extract action flow models from tree-sitter parse trees.

use std::collections::{HashMap, HashSet};
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
    // Post-process: build flow graphs from successions for each action
    for action in &mut results {
        action.steps = build_flow_graph(std::mem::take(&mut action.steps));
    }
    results
}

/// Build a proper flow graph from raw extracted steps.
///
/// The parser extracts fork/join declarations, action usages, and successions
/// as flat lists. This function reconstructs the graph by:
/// 1. Collecting all named nodes (actions, forks, joins)
/// 2. Building an adjacency map from succession sequences
/// 3. Walking from "start" through the graph, nesting fork branches
fn build_flow_graph(raw_steps: Vec<ActionStep>) -> Vec<ActionStep> {
    // Collect named nodes: name → step kind
    let mut node_kinds: HashMap<String, &str> = HashMap::new();
    let mut fork_spans: HashMap<String, Span> = HashMap::new();
    let mut join_spans: HashMap<String, Span> = HashMap::new();
    let mut action_spans: HashMap<String, Span> = HashMap::new();
    // Non-succession, non-declaration steps (if/while/send/accept etc.)
    let mut other_steps: Vec<ActionStep> = Vec::new();

    for step in &raw_steps {
        match step {
            ActionStep::Fork {
                name: Some(n),
                span,
                ..
            } => {
                node_kinds.insert(n.clone(), "fork");
                fork_spans.insert(n.clone(), span.clone());
            }
            ActionStep::Join {
                name: Some(n),
                span,
            } => {
                node_kinds.insert(n.clone(), "join");
                join_spans.insert(n.clone(), span.clone());
            }
            ActionStep::Perform { name, span } => {
                if name != "start" && name != "done" {
                    node_kinds.entry(name.clone()).or_insert("action");
                    action_spans
                        .entry(name.clone())
                        .or_insert_with(|| span.clone());
                }
            }
            _ => {}
        }
    }

    // If no successions found, return raw steps (simple action)
    let has_successions = raw_steps
        .iter()
        .any(|s| matches!(s, ActionStep::Sequence { .. }));
    if !has_successions {
        return raw_steps;
    }

    // Build adjacency: source → [targets]
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    // Also track standalone then_succession targets (from loose `then X;` after a succession)
    // These are the Perform steps that aren't in a Sequence and come after successions
    let mut succession_region = false;

    for step in &raw_steps {
        match step {
            ActionStep::Sequence { steps, .. } if steps.len() == 2 => {
                // `first A then B;` parsed as Sequence [Perform(A), Perform(B)]
                let src = match &steps[0] {
                    ActionStep::Perform { name, .. } => name.clone(),
                    _ => continue,
                };
                let tgt = match &steps[1] {
                    ActionStep::Perform { name, .. } => name.clone(),
                    _ => continue,
                };
                adj.entry(src).or_default().push(tgt);
                succession_region = true;
            }
            ActionStep::Perform { name, .. } if succession_region => {
                // Loose `then X;` — this is a branch target from the preceding succession's source
                // Find the last succession source and add this as an additional target
                // Actually these are `then_succession` parsed as standalone Perform
                // They belong to the PREVIOUS succession's source
                if let Some(last_src) = find_last_succession_source(&raw_steps, step) {
                    adj.entry(last_src).or_default().push(name.clone());
                }
            }
            ActionStep::Fork { .. } | ActionStep::Join { .. } => {
                // Declarations don't reset succession region
            }
            _ => {
                succession_region = false;
                // Collect non-flow steps
                if !matches!(step, ActionStep::Perform { .. }) {
                    other_steps.push(step.clone());
                }
            }
        }
    }

    // Walk the graph from "start" or from root nodes (no incoming edges)
    let mut visited = HashSet::new();
    let result = if adj.contains_key("start") {
        walk_graph(
            "start",
            &adj,
            &node_kinds,
            &fork_spans,
            &join_spans,
            &action_spans,
            &mut visited,
        )
    } else {
        // No explicit "start" — find root nodes (nodes with no incoming edges)
        let mut has_incoming: HashSet<String> = HashSet::new();
        for targets in adj.values() {
            for t in targets {
                has_incoming.insert(t.clone());
            }
        }
        let roots: Vec<String> = adj
            .keys()
            .filter(|k| !has_incoming.contains(*k))
            .cloned()
            .collect();
        if roots.is_empty() {
            // All nodes have incoming edges — just walk from first adj key
            if let Some(first) = adj.keys().next().cloned() {
                walk_graph(
                    &first,
                    &adj,
                    &node_kinds,
                    &fork_spans,
                    &join_spans,
                    &action_spans,
                    &mut visited,
                )
            } else {
                vec![]
            }
        } else {
            let mut result = Vec::new();
            for root in roots {
                result.extend(walk_graph(
                    &root,
                    &adj,
                    &node_kinds,
                    &fork_spans,
                    &join_spans,
                    &action_spans,
                    &mut visited,
                ));
            }
            result
        }
    };

    if result.is_empty() {
        // Fallback: return raw steps if graph walk produced nothing
        return raw_steps;
    }

    let mut final_steps = result;
    // Append non-flow steps (if/while/send/accept that aren't part of the succession graph)
    final_steps.extend(other_steps);
    final_steps
}

fn find_last_succession_source(steps: &[ActionStep], target_step: &ActionStep) -> Option<String> {
    let target_span = match target_step {
        ActionStep::Perform { span, .. } => span,
        _ => return None,
    };
    // Find the succession (Sequence) that immediately precedes this step by span position
    let mut last_src = None;
    for step in steps {
        if let ActionStep::Sequence { steps: seq, .. } = step {
            if seq.len() == 2 {
                if let ActionStep::Perform { span: seq_span, .. } = &seq[0] {
                    if seq_span.end_byte < target_span.start_byte {
                        if let ActionStep::Perform { name, .. } = &seq[0] {
                            // The source of the succession that feeds into this fork
                            // But we actually want the TARGET of that succession (the fork name)
                            if let ActionStep::Perform { name: tgt, .. } = &seq[1] {
                                last_src = Some(tgt.clone());
                            } else {
                                last_src = Some(name.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    last_src
}

fn walk_graph(
    node: &str,
    adj: &HashMap<String, Vec<String>>,
    node_kinds: &HashMap<String, &str>,
    fork_spans: &HashMap<String, Span>,
    join_spans: &HashMap<String, Span>,
    action_spans: &HashMap<String, Span>,
    visited: &mut HashSet<String>,
) -> Vec<ActionStep> {
    if visited.contains(node) || node == "done" {
        return vec![];
    }
    visited.insert(node.to_string());

    let targets = adj.get(node).cloned().unwrap_or_default();

    let kind = node_kinds.get(node).copied().unwrap_or("action");

    match kind {
        "fork" => {
            let span = fork_spans.get(node).cloned().unwrap_or(Span {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 0,
                start_byte: 0,
                end_byte: 0,
            });
            // Each target of the fork is a parallel branch
            // Walk each branch until we hit the matching join
            let join_name = find_matching_join(node, &targets, adj, node_kinds);
            let mut branches = Vec::new();
            for target in &targets {
                let mut branch_steps = walk_branch(
                    target,
                    adj,
                    node_kinds,
                    fork_spans,
                    join_spans,
                    action_spans,
                    visited,
                    join_name.as_deref(),
                );
                if branch_steps.len() == 1 {
                    branches.push(branch_steps.remove(0));
                } else if !branch_steps.is_empty() {
                    branches.push(ActionStep::Sequence {
                        steps: branch_steps,
                        span: span.clone(),
                    });
                }
            }
            let mut result = vec![ActionStep::Fork {
                name: Some(node.to_string()),
                branches,
                span,
            }];
            // Continue after the join
            if let Some(ref jn) = join_name {
                if !visited.contains(jn) {
                    visited.insert(jn.to_string());
                    let jspan = join_spans.get(jn).cloned().unwrap_or(Span {
                        start_row: 0,
                        start_col: 0,
                        end_row: 0,
                        end_col: 0,
                        start_byte: 0,
                        end_byte: 0,
                    });
                    result.push(ActionStep::Join {
                        name: Some(jn.clone()),
                        span: jspan,
                    });
                    let after_join = adj.get(jn).cloned().unwrap_or_default();
                    for aj in after_join {
                        result.extend(walk_graph(
                            &aj,
                            adj,
                            node_kinds,
                            fork_spans,
                            join_spans,
                            action_spans,
                            visited,
                        ));
                    }
                }
            }
            result
        }
        "join" => {
            // Encountered join outside of fork walk — just emit it
            let span = join_spans.get(node).cloned().unwrap_or(Span {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 0,
                start_byte: 0,
                end_byte: 0,
            });
            let mut result = vec![ActionStep::Join {
                name: Some(node.to_string()),
                span,
            }];
            for target in targets {
                result.extend(walk_graph(
                    &target,
                    adj,
                    node_kinds,
                    fork_spans,
                    join_spans,
                    action_spans,
                    visited,
                ));
            }
            result
        }
        _ => {
            // Action node
            let span = action_spans.get(node).cloned().unwrap_or(Span {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 0,
                start_byte: 0,
                end_byte: 0,
            });
            let mut result = vec![ActionStep::Perform {
                name: node.to_string(),
                span,
            }];
            for target in targets {
                result.extend(walk_graph(
                    &target,
                    adj,
                    node_kinds,
                    fork_spans,
                    join_spans,
                    action_spans,
                    visited,
                ));
            }
            result
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_branch(
    node: &str,
    adj: &HashMap<String, Vec<String>>,
    node_kinds: &HashMap<String, &str>,
    fork_spans: &HashMap<String, Span>,
    join_spans: &HashMap<String, Span>,
    action_spans: &HashMap<String, Span>,
    visited: &mut HashSet<String>,
    stop_at_join: Option<&str>,
) -> Vec<ActionStep> {
    if visited.contains(node) || node == "done" {
        return vec![];
    }
    if let Some(jn) = stop_at_join {
        if node == jn {
            return vec![];
        }
    }

    let kind = node_kinds.get(node).copied().unwrap_or("action");

    if kind == "fork" {
        // Nested fork within a branch
        return walk_graph(
            node,
            adj,
            node_kinds,
            fork_spans,
            join_spans,
            action_spans,
            visited,
        );
    }

    if kind == "join" {
        // Hit a join — stop this branch
        return vec![];
    }

    visited.insert(node.to_string());
    let span = action_spans.get(node).cloned().unwrap_or(Span {
        start_row: 0,
        start_col: 0,
        end_row: 0,
        end_col: 0,
        start_byte: 0,
        end_byte: 0,
    });
    let mut result = vec![ActionStep::Perform {
        name: node.to_string(),
        span,
    }];
    let targets = adj.get(node).cloned().unwrap_or_default();
    for target in targets {
        result.extend(walk_branch(
            &target,
            adj,
            node_kinds,
            fork_spans,
            join_spans,
            action_spans,
            visited,
            stop_at_join,
        ));
    }
    result
}

/// Find the join node that corresponds to a fork by tracing branches
fn find_matching_join(
    _fork_name: &str,
    targets: &[String],
    adj: &HashMap<String, Vec<String>>,
    node_kinds: &HashMap<String, &str>,
) -> Option<String> {
    // Trace from any fork target until we hit a join
    for target in targets {
        let mut cur = target.clone();
        let mut seen = HashSet::new();
        while !seen.contains(&cur) {
            seen.insert(cur.clone());
            let kind = node_kinds.get(&cur).copied().unwrap_or("action");
            if kind == "join" {
                return Some(cur);
            }
            if let Some(nexts) = adj.get(&cur) {
                if let Some(next) = nexts.first() {
                    cur = next.clone();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
    None
}

fn collect_action_nodes(node: Node, source: &[u8], _file: &str, results: &mut Vec<ActionModel>) {
    // Check for unified "definition" node with "action" keyword
    let is_action_def = node.kind() == "action_definition"
        || (node.kind() == "definition" && {
            let mut c = node.walk();
            let found = node
                .children(&mut c)
                .any(|ch| !ch.is_named() && crate::parser::node_text(&ch, source) == "action");
            found
        });

    match node.kind() {
        _ if is_action_def => {
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
            // Check if this is a fork_node with empty branches — collect
            // subsequent then_succession siblings as branches
            if let ActionStep::Fork {
                name,
                branches,
                span,
            } = &step
            {
                if branches.is_empty() {
                    let mut collected_branches = Vec::new();
                    let mut j = i + 1;
                    while j < children.len() {
                        if children[j].kind() == "then_succession" {
                            if let Some(branch_step) = extract_step(&children[j], source) {
                                collected_branches.push(branch_step);
                            }
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    if !collected_branches.is_empty() {
                        steps.push(ActionStep::Fork {
                            name: name.clone(),
                            branches: collected_branches,
                            span: span.clone(),
                        });
                        i = j;
                        continue;
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
        "fork_node" | "join_node" | "merge_node" | "decide_node" | "control_node" => {
            // Determine which kind of control node this is
            let ctrl_keyword = if node.kind() == "control_node" {
                let mut c = node.walk();
                let mut kw = "fork";
                for ch in node.children(&mut c) {
                    if !ch.is_named() {
                        match node_text(&ch, source) {
                            "fork" | "join" | "merge" | "decide" => {
                                kw = match node_text(&ch, source) {
                                    "fork" => "fork",
                                    "join" => "join",
                                    "merge" => "merge",
                                    "decide" => "decide",
                                    _ => "fork",
                                };
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                kw
            } else {
                match node.kind() {
                    "fork_node" => "fork",
                    "join_node" => "join",
                    "merge_node" => "merge",
                    "decide_node" => "decide",
                    _ => "fork",
                }
            };
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(&n, source).to_string());
            match ctrl_keyword {
                "fork" => {
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
                "join" => Some(ActionStep::Join {
                    name,
                    span: Span::from_node(node),
                }),
                "decide" => Some(ActionStep::Decide {
                    name,
                    branches: Vec::new(),
                    span: Span::from_node(node),
                }),
                "merge" => Some(ActionStep::Merge {
                    name,
                    span: Span::from_node(node),
                }),
                _ => None,
            }
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
        assert!(!a.steps.is_empty(), "expected steps, got {}", a.steps.len());
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
        let has_merge = a
            .steps
            .iter()
            .any(|s| matches!(s, ActionStep::Merge { .. }));
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
        assert!(has_terminate, "expected Terminate step, got {:?}", a.steps);
    }

    #[test]
    fn no_actions_in_part_file() {
        let source = "part def Vehicle;";
        let actions = extract_actions("test.sysml", source);
        assert!(actions.is_empty());
    }

    #[test]
    fn extract_fork_with_then_branches() {
        let source = r#"
            action def BoardVehicle {
                action driverGetIn;
                action passengerGetIn;
                fork node forkBoard;
                then driverGetIn;
                then passengerGetIn;
                join node joinBoard;
            }
        "#;
        let actions = extract_actions("test.sysml", source);
        assert!(!actions.is_empty(), "should extract action");
        let a = &actions[0];
        // Should have a Fork step with 2 branches (from then_succession siblings)
        let fork = a
            .steps
            .iter()
            .find(|s| matches!(s, ActionStep::Fork { .. }));
        assert!(fork.is_some(), "expected Fork step, got {:?}", a.steps);
        if let Some(ActionStep::Fork { branches, .. }) = fork {
            assert_eq!(
                branches.len(),
                2,
                "fork should have 2 branches, got {:?}",
                branches
            );
        }
        // Should also have a Join step
        let join = a
            .steps
            .iter()
            .find(|s| matches!(s, ActionStep::Join { .. }));
        assert!(join.is_some(), "expected Join step, got {:?}", a.steps);
    }
}

#[test]
fn extract_fork_join_flow_graph() {
    let source = r#"
action def TransportPassenger {
    action driverGetIn;
    action passengerGetIn;
    action checkSafety;
    action driveToDestination;
    action providePower;
    action monitorSystems;
    action driverGetOut;
    action passengerGetOut;

    fork forkBoard;
    join joinBoard;
    fork forkDrive;
    join joinDrive;
    fork forkExit;
    join joinExit;

    first start then forkBoard;
      then driverGetIn;
      then passengerGetIn;
    first driverGetIn then joinBoard;
    first passengerGetIn then joinBoard;

    first joinBoard then checkSafety;
    first checkSafety then forkDrive;
      then driveToDestination;
      then providePower;
      then monitorSystems;
    first driveToDestination then joinDrive;
    first providePower then joinDrive;
    first monitorSystems then joinDrive;

    first joinDrive then forkExit;
      then driverGetOut;
      then passengerGetOut;
    first driverGetOut then joinExit;
    first passengerGetOut then joinExit;

    first joinExit then done;
}
"#;
    let actions = extract_actions("test.sysml", source);
    assert!(!actions.is_empty(), "should extract action");
    let a = &actions[0];
    let forks: Vec<_> = a
        .steps
        .iter()
        .filter(|s| matches!(s, ActionStep::Fork { .. }))
        .collect();
    let joins: Vec<_> = a
        .steps
        .iter()
        .filter(|s| matches!(s, ActionStep::Join { .. }))
        .collect();
    assert_eq!(forks.len(), 3, "expected 3 fork nodes");
    assert_eq!(joins.len(), 3, "expected 3 join nodes");
    // First fork should have 2 branches (driverGetIn, passengerGetIn)
    if let ActionStep::Fork { branches, .. } = &forks[0] {
        assert_eq!(branches.len(), 2, "forkBoard should have 2 branches");
    }
    // Second fork should have 3 branches (driveToDestination, providePower, monitorSystems)
    if let ActionStep::Fork { branches, .. } = &forks[1] {
        assert_eq!(branches.len(), 3, "forkDrive should have 3 branches");
    }
}
