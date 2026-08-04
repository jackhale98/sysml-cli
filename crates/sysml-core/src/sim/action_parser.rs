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
    results
}

/// Control-flow graph collected from an action body in source order.
#[derive(Default)]
struct FlowGraph {
    /// source node -> outgoing target nodes (in declaration order)
    adj: HashMap<String, Vec<String>>,
    /// node -> kind ("action" | "fork" | "join" | "decide" | "merge")
    kinds: HashMap<String, &'static str>,
    /// node -> source span
    spans: HashMap<String, Span>,
    /// (source, target) -> guard expression from `first S if G then T`
    guards: HashMap<(String, String), crate::sim::expr::Expr>,
    /// Non-flow steps (if/while/assign/send/accept/... outside successions)
    others: Vec<ActionStep>,
}

impl FlowGraph {
    fn has_flow(&self) -> bool {
        !self.adj.is_empty()
    }
}

/// Target of a `then ...;` succession as it appears in source.
enum ThenTarget {
    /// Named element (action / join / merge reference / `done`).
    Name(String),
    /// Anonymous control node keyword: fork / join / merge / decide.
    Control(&'static str, Option<String>),
    /// Something that isn't part of the flow graph (send/accept/...).
    Other,
}

/// Build the control-flow graph from an action body walking children in
/// source order. SysML `then` successions chain from an *anchor*: the most
/// recently declared or targeted flow element. Consecutive `then` lines
/// after a fork or decide fan out from it (parallel branches / decision
/// branches) rather than chaining.
fn build_graph_from_body(body: &Node, source: &[u8]) -> FlowGraph {
    let mut g = FlowGraph::default();
    let mut anchor: Option<String> = None;
    let mut fanout = false; // anchor is a fork/decide: `then` lines fan out
    let mut anon = 0usize;

    let children: Vec<Node> = body.children(&mut body.walk()).collect();
    let mut i = 0;
    while i < children.len() {
        let child = &children[i];
        match child.kind() {
            // `first A then B;` and named `succession s first A then B;`
            "succession_statement" | "succession_usage" => {
                let (first, guard, then) = extract_succession_parts(child, source);
                if let Some(f) = first {
                    g.kinds.entry(f.clone()).or_insert("action");
                    match then {
                        Some(t) => {
                            g.kinds.entry(t.clone()).or_insert("action");
                            if let Some(gd) = guard {
                                g.guards.insert((f.clone(), t.clone()), gd);
                            }
                            g.adj.entry(f).or_default().push(t.clone());
                            fanout = matches!(
                                g.kinds.get(t.as_str()),
                                Some(&"fork") | Some(&"decide")
                            );
                            anchor = Some(t);
                        }
                        None => {
                            // bare `first start;` — sets the anchor
                            anchor = Some(f);
                            fanout = false;
                        }
                    }
                }
            }
            "then_succession" => match then_target(child, source) {
                ThenTarget::Name(t) => {
                    g.kinds.entry(t.clone()).or_insert("action");
                    g.spans
                        .entry(t.clone())
                        .or_insert_with(|| Span::from_node(child));
                    if let Some(a) = &anchor {
                        g.adj.entry(a.clone()).or_default().push(t.clone());
                    }
                    let target_is_fanout =
                        matches!(g.kinds.get(t.as_str()), Some(&"fork") | Some(&"decide"));
                    if target_is_fanout {
                        anchor = Some(t);
                        fanout = true;
                    } else if !fanout {
                        anchor = Some(t);
                    }
                    // fanout anchor (fork/decide) keeps collecting branches
                }
                ThenTarget::Control(kind, name) => {
                    let id = name.unwrap_or_else(|| {
                        anon += 1;
                        format!("{}#{}", kind, anon)
                    });
                    g.kinds.insert(id.clone(), kind);
                    g.spans.insert(id.clone(), Span::from_node(child));
                    if let Some(a) = &anchor {
                        g.adj.entry(a.clone()).or_default().push(id.clone());
                    }
                    anchor = Some(id);
                    fanout = matches!(kind, "fork" | "decide");
                }
                ThenTarget::Other => {
                    if let Some(step) = extract_step(child, source) {
                        g.others.push(step);
                    }
                }
            },
            "action_usage" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source).to_string();
                    g.kinds.entry(name.clone()).or_insert("action");
                    g.spans
                        .entry(name.clone())
                        .or_insert_with(|| Span::from_node(child));
                    // A declaration re-anchors the `then` chain
                    anchor = Some(name);
                    fanout = false;
                }
            }
            "control_node" => {
                let (kind, name) = control_node_parts(child, source);
                let id = name.unwrap_or_else(|| {
                    anon += 1;
                    format!("{}#{}", kind, anon)
                });
                g.kinds.insert(id.clone(), kind);
                g.spans.insert(id.clone(), Span::from_node(child));
                anchor = Some(id);
                fanout = matches!(kind, "fork" | "decide");
            }
            "line_comment" | "block_comment" | "{" | "}" => {}
            _ => {
                if let Some(step) = extract_step(child, source) {
                    // if/else pairing as in flat extraction
                    if matches!(step, ActionStep::IfAction { .. }) {
                        if let Some(next) = children.get(i + 1) {
                            if next.kind() == "else_action" {
                                if let ActionStep::IfAction {
                                    condition,
                                    then_step,
                                    span,
                                    ..
                                } = step
                                {
                                    g.others.push(ActionStep::IfAction {
                                        condition,
                                        then_step,
                                        else_step: extract_else_action(next, source)
                                            .map(Box::new),
                                        span,
                                    });
                                    i += 2;
                                    continue;
                                }
                            }
                        }
                    }
                    g.others.push(step);
                }
            }
        }
        i += 1;
    }
    g
}

/// Extract `first A [if G] then B` parts from a succession_statement.
fn extract_succession_parts(
    node: &Node,
    source: &[u8],
) -> (
    Option<String>,
    Option<crate::sim::expr::Expr>,
    Option<String>,
) {
    let mut names: Vec<String> = Vec::new();
    let mut guard = None;
    let mut after_if = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "if" => after_if = true,
            "identifier" | "qualified_name" | "feature_chain" if !after_if => {
                names.push(node_text(&child, source).to_string());
            }
            k if after_if && guard.is_none() && k != "then" => {
                guard = extract_expr(&child, source).ok();
                after_if = false;
            }
            "then" => after_if = false,
            _ => {}
        }
    }
    // `succession s first A then B` includes the succession's own name —
    // when three names are present, the first is the label.
    match names.len() {
        0 => (None, guard, None),
        1 => (Some(names.remove(0)), guard, None),
        2 => {
            let b = names.pop();
            (Some(names.remove(0)), guard, b)
        }
        _ => {
            let b = names.pop();
            let a = names.pop();
            (a, guard, b)
        }
    }
}

/// Classify a then_succession's target.
fn then_target(node: &Node, source: &[u8]) -> ThenTarget {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    // Control keyword targets: `then fork;`, `then merge m;`, ...
    for kw in ["fork", "join", "merge", "decide"] {
        if children
            .iter()
            .any(|c| !c.is_named() && node_text(c, source) == kw)
        {
            let name = children
                .iter()
                .filter(|c| matches!(c.kind(), "identifier" | "qualified_name"))
                .map(|c| node_text(c, source).to_string())
                .next_back();
            let kind: &'static str = match kw {
                "fork" => "fork",
                "join" => "join",
                "merge" => "merge",
                _ => "decide",
            };
            return ThenTarget::Control(kind, name);
        }
    }
    // Behavioral payloads are not flow-graph nodes
    if children.iter().any(|c| {
        matches!(c.kind(), "accept_clause" | "terminate_statement")
            || (!c.is_named()
                && matches!(node_text(c, source), "send" | "assign" | "while" | "if"))
    }) {
        return ThenTarget::Other;
    }
    // Named target: first name-like child
    for c in &children {
        if matches!(c.kind(), "identifier" | "qualified_name" | "feature_chain") {
            return ThenTarget::Name(node_text(c, source).to_string());
        }
    }
    ThenTarget::Other
}

/// (kind, name) of a control_node declaration.
fn control_node_parts(node: &Node, source: &[u8]) -> (&'static str, Option<String>) {
    let mut kind: &'static str = "fork";
    let mut cursor = node.walk();
    for ch in node.children(&mut cursor) {
        if !ch.is_named() {
            match node_text(&ch, source) {
                "fork" => kind = "fork",
                "join" => kind = "join",
                "merge" => kind = "merge",
                "decide" => kind = "decide",
                _ => {}
            }
        }
    }
    let name = node
        .child_by_field_name("name")
        .map(|n| node_text(&n, source).to_string());
    (kind, name)
}

/// Structure a FlowGraph into nested ActionSteps starting from `start`.
fn structure_graph(g: &FlowGraph) -> Vec<ActionStep> {
    let mut visited = HashSet::new();
    let start: Option<String> = if g.adj.contains_key("start") {
        Some("start".to_string())
    } else {
        // Root = node with no incoming edges
        let mut has_incoming: HashSet<&str> = HashSet::new();
        for targets in g.adj.values() {
            for t in targets {
                has_incoming.insert(t.as_str());
            }
        }
        g.adj
            .keys()
            .find(|k| !has_incoming.contains(k.as_str()))
            .cloned()
    };
    let mut steps = match start {
        Some(s) => walk_graph(
            &s,
            &g.adj,
            &g.kinds,
            &g.spans,
            &g.guards,
            &mut visited,
        ),
        None => Vec::new(),
    };
    // Declared join/merge nodes never reached by an edge still appear
    // (declaration-only models); keep source order via spans.
    let mut leftovers: Vec<(&String, &'static str)> = g
        .kinds
        .iter()
        .filter(|(n, k)| matches!(**k, "join" | "merge") && !visited.contains(*n))
        .map(|(n, k)| (n, *k))
        .collect();
    leftovers.sort_by_key(|(n, _)| {
        g.spans.get(*n).map(|s| s.start_byte).unwrap_or(usize::MAX)
    });
    for (n, k) in leftovers {
        let span = g.spans.get(n).cloned().unwrap_or_default();
        steps.push(if k == "join" {
            ActionStep::Join {
                name: Some(n.clone()),
                span,
            }
        } else {
            ActionStep::Merge {
                name: Some(n.clone()),
                span,
            }
        });
    }
    steps.extend(g.others.iter().cloned());
    steps
}

fn walk_graph(
    node: &str,
    adj: &HashMap<String, Vec<String>>,
    kinds: &HashMap<String, &'static str>,
    spans: &HashMap<String, Span>,
    guards: &HashMap<(String, String), crate::sim::expr::Expr>,
    visited: &mut HashSet<String>,
) -> Vec<ActionStep> {
    if visited.contains(node) {
        return vec![];
    }
    visited.insert(node.to_string());

    let targets = adj.get(node).cloned().unwrap_or_default();
    let kind = kinds.get(node).copied().unwrap_or("action");
    let span = spans.get(node).cloned().unwrap_or_default();

    match kind {
        "fork" | "decide" => {
            let converge_kind = if kind == "fork" { "join" } else { "merge" };
            let converge = find_converge(&targets, adj, kinds, converge_kind);
            let mut result = Vec::new();

            if kind == "fork" {
                let mut branches = Vec::new();
                for target in &targets {
                    let mut branch_steps = walk_branch(
                        target,
                        adj,
                        kinds,
                        spans,
                        guards,
                        visited,
                        converge.as_deref(),
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
                result.push(ActionStep::Fork {
                    name: Some(node.to_string()),
                    branches,
                    span,
                });
            } else {
                let mut branches = Vec::new();
                for target in &targets {
                    let steps = walk_branch(
                        target,
                        adj,
                        kinds,
                        spans,
                        guards,
                        visited,
                        converge.as_deref(),
                    );
                    branches.push(DecideBranch {
                        guard: guards.get(&(node.to_string(), target.clone())).cloned(),
                        target: target.clone(),
                        steps,
                    });
                }
                result.push(ActionStep::Decide {
                    name: Some(node.to_string()),
                    branches,
                    span,
                });
            }

            // Continue after the converge node
            if let Some(cv) = converge {
                if !visited.contains(&cv) {
                    visited.insert(cv.clone());
                    let cspan = spans.get(&cv).cloned().unwrap_or_default();
                    result.push(if converge_kind == "join" {
                        ActionStep::Join {
                            name: Some(cv.clone()),
                            span: cspan,
                        }
                    } else {
                        ActionStep::Merge {
                            name: Some(cv.clone()),
                            span: cspan,
                        }
                    });
                    for after in adj.get(&cv).cloned().unwrap_or_default() {
                        result.extend(walk_graph(&after, adj, kinds, spans, guards, visited));
                    }
                }
            }
            result
        }
        "join" | "merge" => {
            let mut result = vec![if kind == "join" {
                ActionStep::Join {
                    name: Some(node.to_string()),
                    span,
                }
            } else {
                ActionStep::Merge {
                    name: Some(node.to_string()),
                    span,
                }
            }];
            for target in targets {
                result.extend(walk_graph(&target, adj, kinds, spans, guards, visited));
            }
            result
        }
        _ => {
            let mut result = vec![ActionStep::Perform {
                name: node.to_string(),
                span,
            }];
            for target in targets {
                result.extend(walk_graph(&target, adj, kinds, spans, guards, visited));
            }
            result
        }
    }
}

/// Walk a branch until the converge node (join/merge) is reached.
#[allow(clippy::too_many_arguments)]
fn walk_branch(
    node: &str,
    adj: &HashMap<String, Vec<String>>,
    kinds: &HashMap<String, &'static str>,
    spans: &HashMap<String, Span>,
    guards: &HashMap<(String, String), crate::sim::expr::Expr>,
    visited: &mut HashSet<String>,
    stop_at: Option<&str>,
) -> Vec<ActionStep> {
    if visited.contains(node) {
        return vec![];
    }
    if stop_at == Some(node) {
        return vec![];
    }
    let kind = kinds.get(node).copied().unwrap_or("action");
    if matches!(kind, "join" | "merge") {
        // Different converge node than expected — stop the branch anyway.
        return vec![];
    }
    if matches!(kind, "fork" | "decide") {
        // Nested fork/decide within a branch
        return walk_graph(node, adj, kinds, spans, guards, visited);
    }

    visited.insert(node.to_string());
    let span = spans.get(node).cloned().unwrap_or_default();
    let mut result = vec![ActionStep::Perform {
        name: node.to_string(),
        span,
    }];
    for target in adj.get(node).cloned().unwrap_or_default() {
        result.extend(walk_branch(
            &target, adj, kinds, spans, guards, visited, stop_at,
        ));
    }
    result
}

/// Find the join/merge node where a fork/decide's branches converge.
fn find_converge(
    targets: &[String],
    adj: &HashMap<String, Vec<String>>,
    kinds: &HashMap<String, &'static str>,
    converge_kind: &str,
) -> Option<String> {
    for target in targets {
        let mut cur = target.clone();
        let mut seen = HashSet::new();
        while !seen.contains(&cur) {
            seen.insert(cur.clone());
            if kinds.get(&cur).copied() == Some(converge_kind) {
                return Some(cur);
            }
            match adj.get(&cur).and_then(|n| n.first()) {
                Some(next) => cur = next.clone(),
                None => break,
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
                        let graph = build_graph_from_body(&child, source);
                        if graph.has_flow() {
                            steps = structure_graph(&graph);
                        } else {
                            extract_action_body(&child, source, &mut steps);
                        }
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
                            let graph = build_graph_from_body(&child, source);
                            if graph.has_flow() {
                                steps = structure_graph(&graph);
                            } else {
                                extract_action_body(&child, source, &mut steps);
                            }
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
                fork forkBoard;
                then driverGetIn;
                then passengerGetIn;
                join joinBoard;
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
