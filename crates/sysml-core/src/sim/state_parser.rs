//! Extract state machine models from tree-sitter parse trees.

use tree_sitter::{Node, Parser};

use crate::model::Span;
use crate::parser::{get_language, node_text};
use crate::sim::expr_parser::extract_expr;
use crate::sim::state_machine::*;

/// Extract all state machine definitions from source.
pub fn extract_state_machines(file: &str, source: &str) -> Vec<StateMachineModel> {
    let mut parser = Parser::new();
    parser.set_language(&get_language()).unwrap();
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let source_bytes = source.as_bytes();
    let mut results = Vec::new();
    collect_state_defs(tree.root_node(), source_bytes, file, &mut results);
    results
}

fn collect_state_defs(
    node: Node,
    source: &[u8],
    _file: &str,
    results: &mut Vec<StateMachineModel>,
) {
    match node.kind() {
        // Standard: state def X { ... }
        "state_definition" => {
            if let Some(sm) = extract_state_machine_from_node(&node, source) {
                results.push(sm);
            }
        }
        // Exhibit: exhibit state X { ... } inside part def bodies
        "exhibit_statement" => {
            if let Some(sm) = extract_state_machine_from_exhibit(&node, source) {
                results.push(sm);
            }
        }
        // Nested state region: state X { states...; transitions...; }
        // A state_usage with its own state_body containing states/transitions
        // is a sub-machine (e.g. operatingStates inside vehicleStates)
        "state_usage" => {
            if let Some(sm) = extract_state_machine_from_state_usage(&node, source) {
                results.push(sm);
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_state_defs(child, source, _file, results);
    }
}

/// Extract a state machine from a `state_definition` node.
fn extract_state_machine_from_node(node: &Node, source: &[u8]) -> Option<StateMachineModel> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(&name_node, source).to_string();
    let mut states = Vec::new();
    let mut transitions = Vec::new();
    let mut entry_state = None;

    let mut variables = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "state_body" || child.kind() == "definition_body" {
            extract_state_body(
                &child,
                source,
                &mut states,
                &mut transitions,
                &mut entry_state,
            );
            variables.extend(extract_variables(&child, source));
        }
    }

    Some(StateMachineModel {
        name,
        states,
        transitions,
        entry_state,
        regions: Vec::new(),
        variables,
        span: Span::from_node(node),
    })
}

/// Extract a state machine from a `state_usage` node that has its own state_body.
///
/// Handles nested state regions like:
///   state operatingStates {
///       entry action initial;
///       state off;
///       state on;
///       transition initial then off;
///       transition off_to_on first off accept StartSignal then on;
///   }
///
/// Only produces a StateMachineModel if the state_usage contains at least
/// one child state or transition (i.e., it's a real state region, not a
/// leaf state like `state off;`).
fn extract_state_machine_from_state_usage(node: &Node, source: &[u8]) -> Option<StateMachineModel> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(&name_node, source).to_string();

    let is_parallel = {
        let mut c = node.walk();
        let kids: Vec<_> = node.children(&mut c).collect();
        kids.iter()
            .any(|ch| !ch.is_named() && node_text(ch, source) == "parallel")
    };

    let mut states = Vec::new();
    let mut transitions = Vec::new();
    let mut entry_state = None;
    let mut regions = Vec::new();
    let mut variables = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "state_body" || child.kind() == "definition_body" {
            extract_state_body(
                &child,
                source,
                &mut states,
                &mut transitions,
                &mut entry_state,
            );
            variables.extend(extract_variables(&child, source));
            if is_parallel {
                // Direct child states with their own bodies are the
                // orthogonal regions.
                let mut bc = child.walk();
                for body_child in child.children(&mut bc) {
                    if body_child.kind() == "state_usage" {
                        if let Some(region) =
                            extract_state_machine_from_state_usage(&body_child, source)
                        {
                            regions.push(region);
                        }
                    }
                }
            }
        }
    }

    // Only return if this state_usage is actually a state region
    // (has child states or transitions), not a leaf state
    if states.is_empty() && transitions.is_empty() {
        return None;
    }

    Some(StateMachineModel {
        name,
        states,
        transitions,
        entry_state,
        regions,
        variables,
        span: Span::from_node(node),
    })
}

/// Extract a state machine from an `exhibit_statement` node.
///
/// An exhibit_statement looks like:
///   exhibit state controllerStates parallel {
///       state operatingStates { ... }
///   }
///
/// The exhibit itself gets a name, and its state_body may contain
/// nested state_usage nodes that are themselves state regions (with
/// their own states and transitions).  We extract both:
///   1. The top-level exhibit as a state machine (if it has states directly)
///   2. Each nested state_usage that has a state_body (as a sub-machine)
fn extract_state_machine_from_exhibit(node: &Node, source: &[u8]) -> Option<StateMachineModel> {
    // Get the exhibit name from the first qualified_name/identifier child
    let mut name = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "qualified_name" || child.kind() == "identifier" {
            let text = node_text(&child, source).to_string();
            if text != "exhibit" && text != "state" {
                name = Some(text);
                break;
            }
        }
    }
    let name = name?;

    let mut states = Vec::new();
    let mut transitions = Vec::new();
    let mut entry_state = None;

    // The owning definition's attributes are in scope of the exhibited
    // state (`part def Drone { attribute soc; exhibit state modes {..} }`).
    let mut variables = node
        .parent()
        .map(|body| extract_variables(&body, source))
        .unwrap_or_default();

    // Find state_body in the exhibit
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "state_body" {
            extract_state_body(
                &child,
                source,
                &mut states,
                &mut transitions,
                &mut entry_state,
            );
            variables.extend(extract_variables(&child, source));
        }
    }

    Some(StateMachineModel {
        name,
        states,
        transitions,
        entry_state,
        regions: Vec::new(),
        variables,
        span: Span::from_node(node),
    })
}

pub(crate) fn extract_state_body(
    body: &Node,
    source: &[u8],
    states: &mut Vec<StateNode>,
    transitions: &mut Vec<Transition>,
    entry_state: &mut Option<String>,
) {
    // Pass 1: collect declared states so `transition A then B;` can be
    // disambiguated — leading name that IS a state is a source shorthand
    // (Ch 28); one that isn't (e.g. `initial`) is an initial
    // pseudo-transition.
    let mut known_states: std::collections::HashSet<String> = std::collections::HashSet::new();
    {
        let mut c = body.walk();
        for child in body.children(&mut c) {
            if child.kind() == "state_usage" {
                if let Some(n) = child.child_by_field_name("name") {
                    known_states.insert(node_text(&n, source).to_string());
                }
            }
        }
    }
    // The most recent state declared in this body: the source of a target
    // transition that omits `first` (`state a; accept Go then b;`).
    let mut previous_state: Option<String> = None;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "state_usage" => {
                if let Some(state) = extract_state_node(&child, source) {
                    previous_state = Some(state.name.clone());
                    states.push(state);
                }
            }
            "entry_action" => {
                // entry; then StateName; — sets the initial state
                let mut ec = child.walk();
                let mut saw_then = false;
                for entry_child in child.children(&mut ec) {
                    if entry_child.kind() == "then" {
                        saw_then = true;
                    } else if saw_then
                        && (entry_child.kind() == "identifier"
                            || entry_child.kind() == "qualified_name")
                    {
                        *entry_state = Some(node_text(&entry_child, source).to_string());
                        break;
                    }
                }
            }
            "transition_statement" => {
                if let Some(t) = extract_transition(&child, source, &known_states) {
                    transitions.push(t);
                    continue;
                }
                let target = extract_initial_transition_target(&child, source);
                let has_trigger_or_guard = {
                    let mut c = child.walk();
                    let kids: Vec<_> = child.children(&mut c).collect();
                    kids.iter()
                        .any(|k| k.kind() == "accept_clause" || k.kind() == "if")
                };
                if entry_state.is_none() && !has_trigger_or_guard {
                    // Handle initial pseudo-transition: `transition initial then off;`
                    // (has `then` target but no `first` source)
                    if let Some(target) = target {
                        *entry_state = Some(target);
                    }
                } else if let Some(prev) = &previous_state {
                    // `transition accept Go then b;` after `state a;`
                    if let Some(t) = extract_transition_from(&child, source, prev) {
                        transitions.push(t);
                    }
                }
            }
            // `accept Go then b;` after `state a;` parses as an accept action.
            "accept_action" => {
                if let Some(prev) = &previous_state {
                    if let Some(t) = extract_accept_then(&child, source, prev) {
                        transitions.push(t);
                    }
                }
            }
            "inline_transition" => {
                if let Some(prev) = &previous_state {
                    if let Some(t) = extract_transition_from(&child, source, prev) {
                        transitions.push(t);
                    }
                }
            }
            "succession_usage" | "succession_statement" => {
                // first X then Y; — shorthand transition
                if let Some(t) = extract_succession_as_transition(&child, source) {
                    transitions.push(t);
                }
            }
            _ => {}
        }
    }
}

/// `accept Sig then target;` in a state body: a transition from `from`.
fn extract_accept_then(node: &Node, source: &[u8], from: &str) -> Option<Transition> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    let then = children
        .iter()
        .position(|c| !c.is_named() && node_text(c, source) == "then")?;
    let target = children[then + 1..].iter().find(|c| c.is_named())?;
    let signal = children[..then]
        .iter()
        .filter(|c| c.kind() == "typed_by")
        .find_map(|c| c.child_by_field_name("type"))
        .or_else(|| {
            children[..then]
                .iter()
                .find(|c| matches!(c.kind(), "qualified_name" | "identifier"))
                .copied()
        })?;
    Some(Transition {
        name: None,
        source: from.to_string(),
        target: node_text(target, source).to_string(),
        trigger: Some(Trigger::Signal(node_text(&signal, source).to_string())),
        guard: None,
        guard_text: None,
        effect: None,
        span: Span::from_node(node),
    })
}

/// A transition with an implied source (target transition after a state).
fn extract_transition_from(node: &Node, source: &[u8], from: &str) -> Option<Transition> {
    let mut t = extract_transition_parts(node, source)?;
    if t.source.is_empty() {
        t.source = from.to_string();
    }
    Some(t)
}

/// Attributes declared directly in a body (`[in] attribute x : T = v;`).
pub(crate) fn extract_variables(body: &Node, source: &[u8]) -> Vec<StateVariable> {
    let mut vars = Vec::new();
    let mut previous: Option<Node> = None;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "usage" && is_attribute_usage(&child, source) {
            if let Some(name) = child.child_by_field_name("name") {
                let direction = previous
                    .filter(|p| p.kind() == "modifier")
                    .map(|p| node_text(&p, source).trim().to_string())
                    .filter(|d| matches!(d.as_str(), "in" | "out" | "inout"));
                let mut type_ref = None;
                let mut value = None;
                let mut expr = None;
                let mut c = child.walk();
                for part in child.children(&mut c) {
                    match part.kind() {
                        "typed_by" => {
                            type_ref = part
                                .child_by_field_name("type")
                                .map(|t| node_text(&t, source).to_string());
                        }
                        "value_assignment" => {
                            let mut vc = part.walk();
                            let value_node = part.children(&mut vc).find(|n| n.is_named());
                            if let Some(e) = value_node {
                                value = Some(node_text(&e, source).trim().to_string());
                                expr = extract_expr(&e, source).ok();
                            }
                        }
                        _ => {}
                    }
                }
                vars.push(StateVariable {
                    name: node_text(&name, source).to_string(),
                    type_ref,
                    value,
                    expr,
                    direction,
                    doc: crate::parser::get_doc_comment(&child, source),
                    span: Span::from_node(&child),
                });
            }
        }
        if child.is_named() {
            previous = Some(child);
        }
    }
    vars
}

fn is_attribute_usage(node: &Node, source: &[u8]) -> bool {
    let mut c = node.walk();
    let found = node
        .children(&mut c)
        .take_while(|k| k.kind() != "identifier")
        .any(|k| !k.is_named() && node_text(&k, source) == "attribute");
    found
}

fn extract_state_node(node: &Node, source: &[u8]) -> Option<StateNode> {
    let name = node
        .child_by_field_name("name")
        .map(|n| node_text(&n, source).to_string())?;

    let mut entry_action = None;
    let mut do_action = None;
    let mut exit_action = None;

    // Walk children for state_body or definition_body
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "state_body" || child.kind() == "definition_body" {
            let mut bc = child.walk();
            for body_child in child.children(&mut bc) {
                match body_child.kind() {
                    "entry_action" => {
                        entry_action = extract_action_ref(&body_child, source);
                    }
                    "do_action" => {
                        do_action = extract_action_ref(&body_child, source);
                    }
                    "exit_action" => {
                        exit_action = extract_action_ref(&body_child, source);
                    }
                    _ => {}
                }
            }
        }
    }

    Some(StateNode {
        name,
        entry_action,
        do_action,
        exit_action,
        span: Span::from_node(node),
    })
}

fn extract_action_ref(node: &Node, source: &[u8]) -> Option<ActionRef> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    // `entry assign x := e;` / `do assign x := e;`
    if let Some(pos) = children
        .iter()
        .position(|c| !c.is_named() && node_text(c, source) == "assign")
    {
        let target = children[pos + 1..]
            .iter()
            .find(|c| c.is_named())
            .map(|c| node_text(c, source).to_string())?;
        let value = children
            .iter()
            .skip_while(|c| node_text(c, source) != ":=")
            .skip(1)
            .find(|c| c.is_named())?;
        return Some(ActionRef::Assign(vec![Assignment {
            target,
            value: node_text(value, source).trim().to_string(),
            expr: extract_expr(value, source).ok(),
        }]));
    }
    for child in &children {
        match child.kind() {
            // `entry; then x;` — the `then` names the initial substate,
            // not an action.
            "then" => return None,
            "identifier" | "qualified_name" => {
                let text = node_text(child, source).to_string();
                // Skip keywords like "entry", "do", "exit", "action", "then", "send"
                if !matches!(
                    text.as_str(),
                    "entry" | "do" | "exit" | "action" | "then" | "send"
                ) {
                    return Some(ActionRef::Named(text));
                }
            }
            "send_action" => {
                return Some(extract_send_action(child, source));
            }
            // `entry action { assign x := 0; }`
            "definition_body" | "action_body" => {
                let mut assignments = Vec::new();
                let mut bc = child.walk();
                for item in child.children(&mut bc) {
                    if item.kind() == "assign_action" {
                        if let Some(ActionRef::Assign(mut a)) = extract_action_ref(&item, source) {
                            assignments.append(&mut a);
                        }
                    }
                }
                if !assignments.is_empty() {
                    return Some(ActionRef::Assign(assignments));
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse an effect from its source text (the part between `do` and
/// `then`). Text, not nodes, because the grammar does not accept every
/// effect form (`do assign x := e` parses with an error node).
fn parse_effect_text(text: &str) -> Option<ActionRef> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text
        .split_whitespace()
        .any(|w| w == "assign" || w == "{assign")
    {
        let assignments: Vec<Assignment> = text
            .split(';')
            .filter_map(|part| {
                let part = part
                    .trim()
                    .trim_start_matches("action")
                    .trim()
                    .trim_start_matches('{')
                    .trim()
                    .trim_end_matches('}')
                    .trim();
                let rest = part.strip_prefix("assign")?;
                let (target, value) = rest.split_once(":=")?;
                let value = value.trim().to_string();
                Some(Assignment {
                    target: target.trim().to_string(),
                    expr: crate::sim::expr_parser::parse_expr_str(&value).ok(),
                    value,
                })
            })
            .collect();
        if !assignments.is_empty() {
            return Some(ActionRef::Assign(assignments));
        }
    }
    let send = text
        .strip_prefix("action")
        .map(str::trim_start)
        .unwrap_or(text);
    if let Some(rest) = send.strip_prefix("send") {
        let mut payload = rest.trim();
        let mut to = None;
        let mut via = None;
        if let Some((p, t)) = payload.rsplit_once(" to ") {
            to = Some(t.trim().to_string());
            payload = p.trim();
        }
        if let Some((p, v)) = payload.rsplit_once(" via ") {
            via = Some(v.trim().to_string());
            payload = p.trim();
        }
        return Some(ActionRef::Send {
            payload: (!payload.is_empty()).then(|| payload.to_string()),
            via,
            to,
        });
    }
    let named = text.strip_prefix("action ").map(str::trim).unwrap_or(text);
    let is_name = named
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == ':' || c == '.' || c == '\'');
    if is_name {
        Some(ActionRef::Named(named.to_string()))
    } else {
        Some(ActionRef::Inline(text.to_string()))
    }
}

fn extract_send_action(node: &Node, source: &[u8]) -> ActionRef {
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

    ActionRef::Send { payload, via, to }
}

fn extract_transition(
    node: &Node,
    source: &[u8],
    known_states: &std::collections::HashSet<String>,
) -> Option<Transition> {
    let mut t = extract_transition_parts(node, source)?;

    // `transition stabilizing then moving;` — without a `first` clause a
    // leading name that names a DECLARED state doubles as the source
    // (book Ch 28 shorthand). Non-state names like `initial` stay as
    // pseudo-transition names handled by the caller.
    if t.source.is_empty() {
        match t.name.take() {
            Some(n) if known_states.contains(n.as_str()) => t.source = n,
            other => {
                t.name = other;
                return None;
            }
        }
    }
    Some(t)
}

/// The parts of a transition. `source` is empty when there is no `first`.
fn extract_transition_parts(node: &Node, source: &[u8]) -> Option<Transition> {
    let name = node
        .child_by_field_name("name")
        .map(|n| node_text(&n, source).to_string());

    let mut source_state = None;
    let mut target_state = None;
    let mut trigger = None;
    let mut guard = None;
    let mut guard_text = None;
    let mut effect_range: Option<(usize, usize)> = None;

    let mut saw_first = false;
    let mut saw_if = false;
    let mut saw_then = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "first" => saw_first = true,
            "if" => saw_if = true,
            "do" => effect_range = Some((child.end_byte(), child.end_byte())),
            "then" => {
                saw_then = true;
                if let Some((start, end)) = effect_range.as_mut() {
                    if *end == *start {
                        *end = child.start_byte();
                    }
                }
            }
            "accept_clause" => {
                trigger = extract_trigger(&child, source);
            }
            _ if !child.is_named() => {}
            _ if effect_range.is_some_and(|(s, e)| s == e) && !saw_then => {
                // Part of the effect; read as text below.
            }
            "identifier" | "qualified_name" | "feature_chain"
                if saw_then || saw_first && !saw_if =>
            {
                let text = node_text(&child, source).to_string();
                if saw_then {
                    target_state = Some(text);
                    saw_then = false;
                } else {
                    source_state = Some(text);
                    saw_first = false;
                }
            }
            _ if saw_if => {
                guard_text = Some(node_text(&child, source).trim().to_string());
                guard = extract_expr(&child, source).ok();
                saw_if = false;
            }
            _ => {}
        }
    }

    let effect = effect_range
        .filter(|(s, e)| e > s)
        .and_then(|(s, e)| std::str::from_utf8(&source[s..e]).ok())
        .and_then(parse_effect_text);

    Some(Transition {
        name,
        source: source_state.unwrap_or_default(),
        target: target_state?,
        trigger,
        guard,
        guard_text,
        effect,
        span: Span::from_node(node),
    })
}

/// Extract the target from an initial pseudo-transition.
///
/// Handles the pattern: `transition initial then off;`
/// where there is no `first` keyword — only a `then` target.
/// Returns the target state name if found.
fn extract_initial_transition_target(node: &Node, source: &[u8]) -> Option<String> {
    let mut saw_then = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "then" => saw_then = true,
            "identifier" | "qualified_name" | "feature_chain" if saw_then => {
                let text = node_text(&child, source).to_string();
                return Some(text);
            }
            _ => {}
        }
    }
    None
}

fn extract_trigger(node: &Node, source: &[u8]) -> Option<Trigger> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    let keyword = children
        .iter()
        .find(|c| !c.is_named() && node_text(c, source) != "accept")
        .map(|c| node_text(c, source));
    let first_named = children.iter().find(|c| c.is_named());
    match (keyword, first_named) {
        (Some("when"), Some(cond))
            if children.iter().position(|c| c.id() == cond.id()) > Some(1) =>
        {
            return Some(Trigger::Change {
                text: node_text(cond, source).trim().to_string(),
                expr: extract_expr(cond, source).ok(),
            })
        }
        (Some("at" | "after"), Some(cond))
            if children.iter().position(|c| c.id() == cond.id()) > Some(1) =>
        {
            let text = node_text(node, source);
            let text = text.strip_prefix("accept").unwrap_or(text).trim();
            return Some(Trigger::Time(text.to_string()));
        }
        _ => {}
    }
    // `accept sig : Sig via port` — the event is the payload type; a bare
    // `accept Sig` names it directly.
    if let Some(typed) = children.iter().find(|c| c.kind() == "typed_by") {
        if let Some(ty) = typed.child_by_field_name("type") {
            return Some(Trigger::Signal(node_text(&ty, source).to_string()));
        }
    }
    for child in &children {
        if matches!(child.kind(), "identifier" | "qualified_name") {
            let text = node_text(child, source).to_string();
            if text != "accept" && text != "when" && text != "at" && text != "after" {
                return Some(Trigger::Signal(text));
            }
        }
    }
    None
}

fn extract_succession_as_transition(node: &Node, source: &[u8]) -> Option<Transition> {
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
    if refs.len() >= 2 {
        Some(Transition {
            name: None,
            source: refs[0].clone(),
            target: refs[1].clone(),
            trigger: None,
            guard: None,
            guard_text: None,
            effect: None,
            span: Span::from_node(node),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_simple_state_machine() {
        let source = r#"
            state def TrafficLight {
                entry; then red;
                state red;
                state yellow;
                state green;
                transition first red then green;
                transition first green then yellow;
                transition first yellow then red;
            }
        "#;
        let machines = extract_state_machines("test.sysml", source);
        assert_eq!(machines.len(), 1);
        let m = &machines[0];
        assert_eq!(m.name, "TrafficLight");
        assert_eq!(m.states.len(), 3);
        assert_eq!(m.entry_state, Some("red".to_string()));
        assert_eq!(m.transitions.len(), 3);
        assert_eq!(m.transitions[0].source, "red");
        assert_eq!(m.transitions[0].target, "green");
    }

    #[test]
    fn extract_flashlight_states() {
        let source = r#"
            state def FlashlightStates {
                entry; then off;
                state off;
                state on;
                transition off_to_on
                    first off
                    accept switchOn
                    then on;
                transition on_to_off
                    first on
                    accept switchOff
                    then off;
            }
        "#;
        let machines = extract_state_machines("test.sysml", source);
        assert_eq!(machines.len(), 1);
        let m = &machines[0];
        assert_eq!(m.states.len(), 2);
        assert_eq!(m.entry_state, Some("off".to_string()));
        assert_eq!(m.transitions.len(), 2);

        let t0 = &m.transitions[0];
        assert_eq!(t0.name, Some("off_to_on".to_string()));
        assert_eq!(t0.source, "off");
        assert_eq!(t0.target, "on");
        assert!(matches!(&t0.trigger, Some(Trigger::Signal(s)) if s == "switchOn"));

        let t1 = &m.transitions[1];
        assert_eq!(t1.source, "on");
        assert_eq!(t1.target, "off");
        assert!(matches!(&t1.trigger, Some(Trigger::Signal(s)) if s == "switchOff"));
    }

    #[test]
    fn no_state_machines_in_non_state_file() {
        let source = "part def Vehicle;";
        let machines = extract_state_machines("test.sysml", source);
        assert!(machines.is_empty());
    }

    #[test]
    fn extract_annex_a_operating_states() {
        let source = r#"
            part def Vehicle {
                exhibit state vehicleStates parallel {
                    state operatingStates {
                        entry action initial;
                        state off;
                        state starting;
                        state on;

                        transition initial then off;

                        transition off_To_starting
                            first off
                            accept ignitionCmd:IgnitionCmd via ignitionCmdPort
                            then starting;

                        transition starting_To_on
                            first starting
                            accept VehicleOnSignal
                            then on;

                        transition on_To_off
                            first on
                            accept VehicleOffSignal
                            then off;
                    }
                }
            }
        "#;
        let machines = extract_state_machines("test.sysml", source);
        let ops = machines
            .iter()
            .find(|m| m.name == "operatingStates")
            .unwrap();
        assert_eq!(ops.entry_state, Some("off".to_string()));
        assert_eq!(ops.states.len(), 3);
        assert_eq!(ops.transitions.len(), 3);

        // The typed accept `ignitionCmd:IgnitionCmd` should extract
        // the usage name as trigger
        let t0 = &ops.transitions[0];
        assert_eq!(t0.source, "off");
        assert_eq!(t0.target, "starting");
        assert!(t0.trigger.is_some(), "off->starting should have a trigger");

        let t1 = &ops.transitions[1];
        assert_eq!(t1.source, "starting");
        assert_eq!(t1.target, "on");
        assert!(matches!(&t1.trigger, Some(Trigger::Signal(s)) if s == "VehicleOnSignal"));
    }

    #[test]
    fn extract_exhibit_state_and_nested_regions() {
        let source = r#"
            part def VehicleController {
                exhibit state controllerStates parallel {
                    state operatingStates {
                        entry action initial;
                        state off;
                        state on;
                        transition initial then off;
                        transition off_to_on
                            first off
                            accept StartSignal
                            then on;
                        transition on_to_off
                            first on
                            accept OffSignal
                            then off;
                    }
                }
            }
        "#;
        let machines = extract_state_machines("test.sysml", source);
        assert_eq!(machines.len(), 2);

        // Top-level exhibit: controllerStates
        let top = &machines[0];
        assert_eq!(top.name, "controllerStates");
        assert_eq!(top.states.len(), 1);
        assert_eq!(top.states[0].name, "operatingStates");

        // Nested region: operatingStates
        let nested = &machines[1];
        assert_eq!(nested.name, "operatingStates");
        assert_eq!(nested.states.len(), 2);
        assert_eq!(nested.states[0].name, "off");
        assert_eq!(nested.states[1].name, "on");
        assert_eq!(nested.transitions.len(), 2);
        assert_eq!(nested.transitions[0].source, "off");
        assert_eq!(nested.transitions[0].target, "on");
        assert!(
            matches!(&nested.transitions[0].trigger, Some(Trigger::Signal(s)) if s == "StartSignal")
        );
    }

    #[test]
    fn extract_exhibit_state_parallel_with_multiple_regions() {
        // Mirrors annex-a vehicleStates with operatingStates + healthStates
        let source = r#"
            part def Vehicle {
                exhibit state vehicleStates parallel {
                    state operatingStates {
                        entry action initial;
                        state off;
                        state starting;
                        state on;
                        transition initial then off;
                        transition off_To_starting
                            first off
                            accept IgnitionCmd
                            then starting;
                        transition starting_To_on
                            first starting
                            accept VehicleOnSignal
                            then on;
                        transition on_To_off
                            first on
                            accept VehicleOffSignal
                            then off;
                    }
                    state healthStates {
                        entry action initial;
                        state normal;
                        state degraded;
                        transition initial then normal;
                        transition normal_To_degraded
                            first normal
                            accept OverTempSignal
                            then degraded;
                    }
                }
            }
        "#;
        let machines = extract_state_machines("test.sysml", source);
        // Should find: vehicleStates (exhibit), operatingStates, healthStates
        assert_eq!(machines.len(), 3);
        assert_eq!(machines[0].name, "vehicleStates");
        assert_eq!(machines[1].name, "operatingStates");
        assert_eq!(machines[1].states.len(), 3);
        assert_eq!(machines[1].transitions.len(), 3);
        assert_eq!(machines[2].name, "healthStates");
        assert_eq!(machines[2].states.len(), 2);
        assert_eq!(machines[2].transitions.len(), 1);
    }

    #[test]
    fn change_and_time_triggers_guards_and_effects() {
        let source = r#"
            state def M {
                attribute count : Integer = 0;
                in attribute soc : Real;
                entry; then a;
                state a;
                state b;
                transition low first a accept when soc < 0.3 then b;
                transition later first a accept after 5 [s] then b;
                transition typed first a accept sig : Sig via port1 then b;
                transition guarded first b accept Go if count > 1 and soc >= 0.5 then a;
                transition bump first b accept Bump do assign count := count + 2 then b;
                transition say first b accept Say do send Hello() to a then b;
            }
        "#;
        let m = &extract_state_machines("t.sysml", source)[0];
        let by_name = |n: &str| {
            m.transitions
                .iter()
                .find(|t| t.name.as_deref() == Some(n))
                .unwrap()
        };
        assert!(matches!(
            &by_name("low").trigger,
            Some(Trigger::Change { text, expr: Some(_) }) if text == "soc < 0.3"
        ));
        assert!(matches!(&by_name("later").trigger, Some(Trigger::Time(t)) if t == "after 5 [s]"));
        assert!(matches!(&by_name("typed").trigger, Some(Trigger::Signal(s)) if s == "Sig"));
        let guarded = by_name("guarded");
        assert_eq!(
            guarded.guard_text.as_deref(),
            Some("count > 1 and soc >= 0.5")
        );
        assert!(guarded.guard.is_some());
        match &by_name("bump").effect {
            Some(ActionRef::Assign(a)) => {
                assert_eq!(a[0].target, "count");
                assert_eq!(a[0].value, "count + 2");
                assert!(a[0].expr.is_some());
            }
            other => panic!("expected an assignment, got {other:?}"),
        }
        assert!(
            matches!(&by_name("say").effect, Some(ActionRef::Send { to: Some(t), .. }) if t == "a")
        );
        let vars: Vec<_> = m.variables.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(vars, vec!["count", "soc"]);
        assert_eq!(m.variables[1].direction.as_deref(), Some("in"));
    }

    #[test]
    fn target_transitions_take_the_preceding_state() {
        let source = r#"
            state def M {
                entry; then a;
                state a;
                accept Go then b;
                state b;
                transition accept Back then a;
            }
        "#;
        let m = &extract_state_machines("t.sysml", source)[0];
        let pairs: Vec<_> = m
            .transitions
            .iter()
            .map(|t| (t.source.as_str(), t.target.as_str()))
            .collect();
        assert_eq!(pairs, vec![("a", "b"), ("b", "a")]);
        assert_eq!(m.entry_state.as_deref(), Some("a"));
    }

    #[test]
    fn composite_initial_state_is_not_an_entry_action() {
        let source = r#"
            state def M {
                entry; then on;
                state on { entry; then x; state x; }
                state lit { entry assign n := 1; }
            }
        "#;
        let m = &extract_state_machines("t.sysml", source)[0];
        assert!(m.states[0].entry_action.is_none());
        assert!(
            matches!(&m.states[1].entry_action, Some(ActionRef::Assign(a)) if a[0].target == "n")
        );
    }
}
