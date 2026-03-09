/// Diagram builders: convert Model data into DiagramGraph.

use crate::model::*;
use crate::sim::state_machine::StateMachineModel;
use crate::sim::action_flow::{ActionModel, ActionStep};
use super::graph::*;

/// Build a Block Definition Diagram showing definitions and their relationships.
///
/// Shows: definitions as blocks, specialization arrows, composition (part usages).
pub fn build_bdd(model: &Model, scope: Option<&str>) -> DiagramGraph {
    let title = scope
        .map(|s| format!("bdd [{}]", s))
        .unwrap_or_else(|| format!("bdd [{}]", model.file));
    let mut graph = DiagramGraph::new(title, DiagramKind::Bdd);

    let defs: Vec<&Definition> = if let Some(scope_name) = scope {
        // Show the scoped definition and its children
        model
            .definitions
            .iter()
            .filter(|d| d.name == scope_name || d.parent_def.as_deref() == Some(scope_name))
            .collect()
    } else {
        model.definitions.iter().collect()
    };

    // Add definition nodes
    for def in &defs {
        let stereotype = format!("<<{}>>", def.kind.label());
        let mut attrs = Vec::new();
        if def.is_abstract {
            attrs.push(("modifier".to_string(), "abstract".to_string()));
        }

        // Collect child usages as attributes
        for u in model.usages_in_def(&def.name) {
            let t = u.type_ref.as_deref().unwrap_or("?");
            let mult = u
                .multiplicity
                .as_ref()
                .map(|m| format!(" {}", m))
                .unwrap_or_default();
            attrs.push((u.kind.clone(), format!("{} : {}{}", u.name, t, mult)));
        }

        graph.add_node(DiagramNode {
            id: def.name.clone(),
            label: def.name.clone(),
            kind: NodeKind::Block,
            stereotype: Some(stereotype),
            attributes: attrs,
        });
    }

    // Specialization edges
    for def in &defs {
        if let Some(ref st) = def.super_type {
            let target = simple_name(st);
            if graph.has_node(target) {
                graph.add_edge(DiagramEdge {
                    source: def.name.clone(),
                    target: target.to_string(),
                    label: None,
                    kind: EdgeKind::Specialization,
                });
            }
        }
    }

    // Composition edges (part usages that reference other definitions in scope)
    let def_names: std::collections::HashSet<&str> =
        defs.iter().map(|d| d.name.as_str()).collect();
    for def in &defs {
        for u in model.usages_in_def(&def.name) {
            if let Some(ref t) = u.type_ref {
                let t_simple = simple_name(t);
                if def_names.contains(t_simple) && u.kind == "part" {
                    graph.add_edge(DiagramEdge {
                        source: def.name.clone(),
                        target: t_simple.to_string(),
                        label: Some(u.name.clone()),
                        kind: EdgeKind::Composition,
                    });
                }
            }
        }
    }

    graph
}

/// Build an Internal Block Diagram for a specific definition.
///
/// Shows: parts as blocks, ports on edges, connections between ports.
pub fn build_ibd(model: &Model, def_name: &str) -> DiagramGraph {
    let title = format!("ibd [{}]", def_name);
    let mut graph = DiagramGraph::new(title, DiagramKind::Ibd);

    let usages = model.usages_in_def(def_name);

    // Add part usages as blocks
    for u in &usages {
        if u.kind == "part" {
            let t = u.type_ref.as_deref().unwrap_or("?");
            graph.add_node(DiagramNode {
                id: u.name.clone(),
                label: format!("{} : {}", u.name, t),
                kind: NodeKind::Block,
                stereotype: None,
                attributes: Vec::new(),
            });
        } else if u.kind == "port" {
            let t = u.type_ref.as_deref().unwrap_or("?");
            let dir = u
                .direction
                .map(|d| format!("{} ", d.label()))
                .unwrap_or_default();
            graph.add_node(DiagramNode {
                id: u.name.clone(),
                label: format!("{}{} : {}", dir, u.name, t),
                kind: NodeKind::Port,
                stereotype: None,
                attributes: Vec::new(),
            });
        }
    }

    // Add connections
    for conn in &model.connections {
        let src = simple_name(&conn.source);
        let tgt = simple_name(&conn.target);
        // Only include connections between elements in this definition
        if graph.has_node(src) || graph.has_node(tgt) {
            graph.add_edge(DiagramEdge {
                source: src.to_string(),
                target: tgt.to_string(),
                label: conn.name.clone(),
                kind: EdgeKind::Connection,
            });
        }
    }

    // Add flows
    for flow in &model.flows {
        let src = simple_name(&flow.source);
        let tgt = simple_name(&flow.target);
        if graph.has_node(src) || graph.has_node(tgt) {
            graph.add_edge(DiagramEdge {
                source: src.to_string(),
                target: tgt.to_string(),
                label: flow
                    .item_type
                    .as_ref()
                    .map(|t| format!("<<{}>>", t))
                    .or(flow.name.clone()),
                kind: EdgeKind::Flow,
            });
        }
    }

    graph
}

/// Build a State Machine Diagram.
pub fn build_stm(model: &Model, state_def_name: Option<&str>) -> DiagramGraph {
    let state_defs: Vec<&Definition> = model
        .definitions
        .iter()
        .filter(|d| d.kind == DefKind::State)
        .collect();

    let target = if let Some(name) = state_def_name {
        state_defs.iter().find(|d| d.name == name).copied()
    } else {
        state_defs.first().copied()
    };

    let title = target
        .map(|d| format!("stm [{}]", d.name))
        .unwrap_or_else(|| "stm".to_string());
    let mut graph = DiagramGraph::new(title, DiagramKind::Stm);

    let Some(state_def) = target else {
        return graph;
    };

    // Find state usages inside this state def
    let states: Vec<&Usage> = model
        .usages
        .iter()
        .filter(|u| u.kind == "state" && u.parent_def.as_deref() == Some(&state_def.name))
        .collect();

    // Add initial state
    graph.add_node(DiagramNode {
        id: "__initial__".to_string(),
        label: String::new(),
        kind: NodeKind::InitialState,
        stereotype: None,
        attributes: Vec::new(),
    });

    // Add state nodes
    for s in &states {
        graph.add_node(DiagramNode {
            id: s.name.clone(),
            label: s.name.clone(),
            kind: NodeKind::State,
            stereotype: None,
            attributes: Vec::new(),
        });
    }

    // Add initial transition to the first state
    if let Some(first) = states.first() {
        graph.add_edge(DiagramEdge {
            source: "__initial__".to_string(),
            target: first.name.clone(),
            label: None,
            kind: EdgeKind::Transition,
        });
    }

    graph
}

/// Build a Requirements Diagram.
///
/// Shows: requirements, satisfy and verify relationships.
pub fn build_req(model: &Model) -> DiagramGraph {
    let title = format!("req [{}]", model.file);
    let mut graph = DiagramGraph::new(title, DiagramKind::Req);

    // Add requirement nodes
    for def in &model.definitions {
        if def.kind == DefKind::Requirement {
            let mut attrs = Vec::new();
            if let Some(ref doc) = def.doc {
                attrs.push(("text".to_string(), doc.clone()));
            }
            graph.add_node(DiagramNode {
                id: def.name.clone(),
                label: def.name.clone(),
                kind: NodeKind::Requirement,
                stereotype: Some("<<requirement>>".to_string()),
                attributes: attrs,
            });
        }
    }

    // Add blocks that satisfy or verify requirements
    let mut blocks_added = std::collections::HashSet::new();

    // Build a map from satisfaction requirement to enclosing definition
    // for implicit satisfactions (no explicit "by" clause)
    let satisfaction_owners: std::collections::HashMap<usize, &str> = model
        .satisfactions
        .iter()
        .enumerate()
        .filter(|(_, s)| s.by.is_none())
        .filter_map(|(i, s)| {
            // Find which definition contains this satisfy statement by span
            model
                .definitions
                .iter()
                .filter(|d| d.has_body)
                .filter(|d| {
                    d.span.start_byte <= s.span.start_byte
                        && s.span.end_byte <= d.span.end_byte
                })
                .last()
                .map(|d| (i, d.name.as_str()))
        })
        .collect();

    for (i, sat) in model.satisfactions.iter().enumerate() {
        let by_name = if let Some(ref by) = sat.by {
            simple_name(by).to_string()
        } else if let Some(owner) = satisfaction_owners.get(&i) {
            owner.to_string()
        } else {
            continue;
        };

        if !blocks_added.contains(&by_name) {
            graph.add_node(DiagramNode {
                id: by_name.clone(),
                label: by_name.clone(),
                kind: NodeKind::Block,
                stereotype: None,
                attributes: Vec::new(),
            });
            blocks_added.insert(by_name.clone());
        }
        graph.add_edge(DiagramEdge {
            source: by_name,
            target: simple_name(&sat.requirement).to_string(),
            label: Some("<<satisfy>>".to_string()),
            kind: EdgeKind::Satisfy,
        });
    }

    for ver in &model.verifications {
        let by_name = simple_name(&ver.by);
        if !blocks_added.contains(by_name) {
            graph.add_node(DiagramNode {
                id: by_name.to_string(),
                label: by_name.to_string(),
                kind: NodeKind::Block,
                stereotype: None,
                attributes: Vec::new(),
            });
            blocks_added.insert(by_name.to_string());
        }
        graph.add_edge(DiagramEdge {
            source: by_name.to_string(),
            target: simple_name(&ver.requirement).to_string(),
            label: Some("<<verify>>".to_string()),
            kind: EdgeKind::Verify,
        });
    }

    graph
}

/// Build a Package Diagram.
///
/// Shows: packages and their contained definitions.
pub fn build_pkg(model: &Model) -> DiagramGraph {
    let title = format!("pkg [{}]", model.file);
    let mut graph = DiagramGraph::new(title, DiagramKind::Pkg);

    // Packages
    let packages: Vec<&Definition> = model
        .definitions
        .iter()
        .filter(|d| d.kind == DefKind::Package)
        .collect();

    for pkg in &packages {
        graph.add_node(DiagramNode {
            id: pkg.name.clone(),
            label: pkg.name.clone(),
            kind: NodeKind::Package,
            stereotype: Some("<<package>>".to_string()),
            attributes: Vec::new(),
        });

        // Definitions contained in this package
        let children: Vec<&Definition> = model
            .definitions
            .iter()
            .filter(|d| d.parent_def.as_deref() == Some(&pkg.name) && d.kind != DefKind::Package)
            .collect();

        let child_ids: Vec<String> = children.iter().map(|d| d.name.clone()).collect();

        for child in &children {
            graph.add_node(DiagramNode {
                id: child.name.clone(),
                label: child.name.clone(),
                kind: NodeKind::Block,
                stereotype: Some(format!("<<{}>>", child.kind.label())),
                attributes: Vec::new(),
            });
        }

        if !child_ids.is_empty() {
            graph.add_subgraph(Subgraph {
                id: pkg.name.clone(),
                label: pkg.name.clone(),
                node_ids: child_ids,
            });
        }
    }

    // Top-level definitions not in any package
    for def in &model.definitions {
        if def.parent_def.is_none() && def.kind != DefKind::Package {
            graph.add_node(DiagramNode {
                id: def.name.clone(),
                label: def.name.clone(),
                kind: NodeKind::Block,
                stereotype: Some(format!("<<{}>>", def.kind.label())),
                attributes: Vec::new(),
            });
        }
    }

    graph
}

/// Build a Parametric Diagram.
///
/// Shows: constraints and their parameter bindings.
pub fn build_par(model: &Model, scope: Option<&str>) -> DiagramGraph {
    let title = scope
        .map(|s| format!("par [{}]", s))
        .unwrap_or_else(|| format!("par [{}]", model.file));
    let mut graph = DiagramGraph::new(title, DiagramKind::Par);

    let constraint_defs: Vec<&Definition> = model
        .definitions
        .iter()
        .filter(|d| {
            d.kind == DefKind::Constraint
                && (scope.is_none() || d.parent_def.as_deref() == scope || Some(d.name.as_str()) == scope)
        })
        .collect();

    for cdef in &constraint_defs {
        let params: Vec<&Usage> = model
            .usages
            .iter()
            .filter(|u| u.parent_def.as_deref() == Some(&cdef.name))
            .collect();

        let attrs: Vec<(String, String)> = params
            .iter()
            .map(|p| {
                let dir = p
                    .direction
                    .map(|d| format!("{} ", d.label()))
                    .unwrap_or_default();
                let t = p.type_ref.as_deref().unwrap_or("?");
                (format!("{}{}", dir, p.name), t.to_string())
            })
            .collect();

        graph.add_node(DiagramNode {
            id: cdef.name.clone(),
            label: cdef.name.clone(),
            kind: NodeKind::Constraint,
            stereotype: Some("<<constraint>>".to_string()),
            attributes: attrs,
        });
    }

    graph
}

/// Build a rich State Machine Diagram from a parsed `StateMachineModel`.
///
/// Shows: states with entry/do/exit actions, full transition labels
/// (trigger [guard] / effect), initial and final pseudo-states.
pub fn build_stm_from_state_machine(sm: &StateMachineModel) -> DiagramGraph {
    let title = format!("stm [{}]", sm.name);
    let mut graph = DiagramGraph::new(title, DiagramKind::Stm);

    // Initial pseudo-state
    graph.add_node(DiagramNode {
        id: "__initial__".to_string(),
        label: String::new(),
        kind: NodeKind::InitialState,
        stereotype: None,
        attributes: Vec::new(),
    });

    // State nodes with entry/do/exit actions as attributes
    for state in &sm.states {
        let mut attrs = Vec::new();
        if let Some(ref a) = state.entry_action {
            attrs.push(("entry".to_string(), a.to_string()));
        }
        if let Some(ref a) = state.do_action {
            attrs.push(("do".to_string(), a.to_string()));
        }
        if let Some(ref a) = state.exit_action {
            attrs.push(("exit".to_string(), a.to_string()));
        }
        graph.add_node(DiagramNode {
            id: state.name.clone(),
            label: state.name.clone(),
            kind: NodeKind::State,
            stereotype: None,
            attributes: attrs,
        });
    }

    // Initial transition to entry state
    if let Some(ref entry) = sm.entry_state {
        graph.add_edge(DiagramEdge {
            source: "__initial__".to_string(),
            target: entry.clone(),
            label: None,
            kind: EdgeKind::Transition,
        });
    }

    // Transitions with full labels: trigger [guard] / effect
    for tr in &sm.transitions {
        let mut label_parts = Vec::new();
        if let Some(ref trigger) = tr.trigger {
            label_parts.push(trigger.to_string());
        }
        if let Some(ref guard) = tr.guard {
            label_parts.push(format!("[{}]", guard));
        }
        if let Some(ref effect) = tr.effect {
            label_parts.push(format!("/ {}", effect));
        }
        let label = if label_parts.is_empty() {
            None
        } else {
            Some(label_parts.join(" "))
        };

        graph.add_edge(DiagramEdge {
            source: tr.source.clone(),
            target: tr.target.clone(),
            label,
            kind: EdgeKind::Transition,
        });
    }

    graph
}

/// Build an Activity Diagram from a parsed `ActionModel`.
///
/// Shows: actions as rounded boxes, decisions as diamonds, fork/join bars,
/// initial node, final node (done), and control flow edges.
pub fn build_act_from_action_model(am: &ActionModel) -> DiagramGraph {
    let title = format!("act [{}]", am.name);
    let mut graph = DiagramGraph::new(title, DiagramKind::Act);

    // Initial node
    graph.add_node(DiagramNode {
        id: "__initial__".to_string(),
        label: String::new(),
        kind: NodeKind::InitialState,
        stereotype: None,
        attributes: Vec::new(),
    });

    let mut counter = 0usize;
    let mut last_ids: Vec<String> = vec!["__initial__".to_string()];

    for step in &am.steps {
        let new_ids = add_action_step(&mut graph, step, &last_ids, &mut counter);
        last_ids = new_ids;
    }

    // If the last step wasn't a Done, add a final node
    let has_final = graph.nodes.iter().any(|n| n.kind == NodeKind::FinalState);
    if !has_final {
        graph.add_node(DiagramNode {
            id: "__final__".to_string(),
            label: String::new(),
            kind: NodeKind::FinalState,
            stereotype: None,
            attributes: Vec::new(),
        });
        for id in &last_ids {
            graph.add_edge(DiagramEdge {
                source: id.clone(),
                target: "__final__".to_string(),
                label: None,
                kind: EdgeKind::Flow,
            });
        }
    }

    graph
}

fn add_action_step(
    graph: &mut DiagramGraph,
    step: &ActionStep,
    prev_ids: &[String],
    counter: &mut usize,
) -> Vec<String> {
    match step {
        ActionStep::Perform { name, .. } => {
            let id = format!("act_{}", *counter);
            *counter += 1;
            graph.add_node(DiagramNode {
                id: id.clone(),
                label: name.clone(),
                kind: NodeKind::Action,
                stereotype: None,
                attributes: Vec::new(),
            });
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            vec![id]
        }
        ActionStep::Sequence { steps, .. } => {
            let mut ids = prev_ids.to_vec();
            for s in steps {
                ids = add_action_step(graph, s, &ids, counter);
            }
            ids
        }
        ActionStep::Fork { name, branches, .. } => {
            let fork_id = format!("fork_{}", *counter);
            *counter += 1;
            graph.add_node(DiagramNode {
                id: fork_id.clone(),
                label: name.as_deref().unwrap_or("fork").to_string(),
                kind: NodeKind::Fork,
                stereotype: None,
                attributes: Vec::new(),
            });
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: fork_id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            let mut branch_ends = Vec::new();
            for branch in branches {
                let ends = add_action_step(graph, branch, &[fork_id.clone()], counter);
                branch_ends.extend(ends);
            }
            branch_ends
        }
        ActionStep::Join { name, .. } => {
            let join_id = format!("join_{}", *counter);
            *counter += 1;
            graph.add_node(DiagramNode {
                id: join_id.clone(),
                label: name.as_deref().unwrap_or("join").to_string(),
                kind: NodeKind::Join,
                stereotype: None,
                attributes: Vec::new(),
            });
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: join_id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            vec![join_id]
        }
        ActionStep::Decide { name, branches, .. } => {
            let dec_id = format!("decide_{}", *counter);
            *counter += 1;
            graph.add_node(DiagramNode {
                id: dec_id.clone(),
                label: name.as_deref().unwrap_or("?").to_string(),
                kind: NodeKind::Decision,
                stereotype: None,
                attributes: Vec::new(),
            });
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: dec_id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            let mut branch_ends = Vec::new();
            for branch in branches {
                // Create a node for the branch target
                let target_id = format!("act_{}", *counter);
                *counter += 1;
                graph.add_node(DiagramNode {
                    id: target_id.clone(),
                    label: branch.target.clone(),
                    kind: NodeKind::Action,
                    stereotype: None,
                    attributes: Vec::new(),
                });
                let guard_label = branch.guard.as_ref().map(|g| format!("[{}]", g));
                graph.add_edge(DiagramEdge {
                    source: dec_id.clone(),
                    target: target_id.clone(),
                    label: guard_label,
                    kind: EdgeKind::Flow,
                });
                branch_ends.push(target_id);
            }
            branch_ends
        }
        ActionStep::Merge { name, .. } => {
            let merge_id = format!("merge_{}", *counter);
            *counter += 1;
            graph.add_node(DiagramNode {
                id: merge_id.clone(),
                label: name.as_deref().unwrap_or("merge").to_string(),
                kind: NodeKind::Decision, // diamonds for merge too
                stereotype: None,
                attributes: Vec::new(),
            });
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: merge_id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            vec![merge_id]
        }
        ActionStep::IfAction { condition, then_step, else_step, .. } => {
            let dec_id = format!("if_{}", *counter);
            *counter += 1;
            graph.add_node(DiagramNode {
                id: dec_id.clone(),
                label: format!("{}", condition),
                kind: NodeKind::Decision,
                stereotype: None,
                attributes: Vec::new(),
            });
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: dec_id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }

            let mut all_ends = Vec::new();

            // Then branch
            let then_ends = add_action_step(graph, then_step, &[dec_id.clone()], counter);
            // Add guard label on the first edge from decision to then
            if let Some(edge) = graph.edges.iter_mut().rev().find(|e| e.source == dec_id) {
                edge.label = Some("[yes]".to_string());
            }
            all_ends.extend(then_ends);

            // Else branch
            if let Some(else_s) = else_step {
                let else_ends = add_action_step(graph, else_s, &[dec_id.clone()], counter);
                if let Some(edge) = graph.edges.iter_mut().rev().find(|e| e.source == dec_id) {
                    edge.label = Some("[no]".to_string());
                }
                all_ends.extend(else_ends);
            } else {
                all_ends.push(dec_id);
            }
            all_ends
        }
        ActionStep::Assign { target, value, .. } => {
            let id = format!("act_{}", *counter);
            *counter += 1;
            graph.add_node(DiagramNode {
                id: id.clone(),
                label: format!("{} := {}", target, value),
                kind: NodeKind::Action,
                stereotype: None,
                attributes: Vec::new(),
            });
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            vec![id]
        }
        ActionStep::Send { payload, via, to, .. } => {
            let id = format!("act_{}", *counter);
            *counter += 1;
            let mut label = "send".to_string();
            if let Some(p) = payload { label.push_str(&format!(" {}", p)); }
            if let Some(v) = via { label.push_str(&format!(" via {}", v)); }
            if let Some(t) = to { label.push_str(&format!(" to {}", t)); }
            graph.add_node(DiagramNode {
                id: id.clone(),
                label,
                kind: NodeKind::Action,
                stereotype: Some("<<send>>".to_string()),
                attributes: Vec::new(),
            });
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            vec![id]
        }
        ActionStep::WhileLoop { condition, body, .. } => {
            let dec_id = format!("while_{}", *counter);
            *counter += 1;
            graph.add_node(DiagramNode {
                id: dec_id.clone(),
                label: format!("{}", condition),
                kind: NodeKind::Decision,
                stereotype: None,
                attributes: Vec::new(),
            });
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: dec_id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            let body_ends = add_action_step(graph, body, &[dec_id.clone()], counter);
            // Loop back
            for end_id in &body_ends {
                graph.add_edge(DiagramEdge {
                    source: end_id.clone(),
                    target: dec_id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            // Exit edge from decision
            vec![dec_id]
        }
        ActionStep::ForLoop { variable, collection, body, .. } => {
            let dec_id = format!("for_{}", *counter);
            *counter += 1;
            graph.add_node(DiagramNode {
                id: dec_id.clone(),
                label: format!("for {} in {}", variable, collection),
                kind: NodeKind::Decision,
                stereotype: None,
                attributes: Vec::new(),
            });
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: dec_id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            let body_ends = add_action_step(graph, body, &[dec_id.clone()], counter);
            for end_id in &body_ends {
                graph.add_edge(DiagramEdge {
                    source: end_id.clone(),
                    target: dec_id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            vec![dec_id]
        }
        ActionStep::Accept { signal, .. } => {
            let id = format!("accept_{}", *counter);
            *counter += 1;
            let label = match signal {
                Some(s) => format!("accept {}", s),
                None => "accept".to_string(),
            };
            graph.add_node(DiagramNode {
                id: id.clone(),
                label,
                kind: NodeKind::Action,
                stereotype: Some("accept".to_string()),
                attributes: Vec::new(),
            });
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            vec![id]
        }
        ActionStep::Terminate { .. } | ActionStep::Done { .. } => {
            let id = "__final__".to_string();
            if !graph.has_node(&id) {
                graph.add_node(DiagramNode {
                    id: id.clone(),
                    label: String::new(),
                    kind: NodeKind::FinalState,
                    stereotype: None,
                    attributes: Vec::new(),
                });
            }
            for prev in prev_ids {
                graph.add_edge(DiagramEdge {
                    source: prev.clone(),
                    target: id.clone(),
                    label: None,
                    kind: EdgeKind::Flow,
                });
            }
            vec![id]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn bdd_contains_definitions() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle;
            part def Engine;
            part def Wheel;
        "#,
        );
        let graph = build_bdd(&model, None);
        assert_eq!(graph.kind, DiagramKind::Bdd);
        assert_eq!(graph.nodes.len(), 3);
        assert!(graph.has_node("Vehicle"));
        assert!(graph.has_node("Engine"));
    }

    #[test]
    fn bdd_specialization_edge() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def PowerSource;
            part def Engine :> PowerSource;
        "#,
        );
        let graph = build_bdd(&model, None);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].kind, EdgeKind::Specialization);
        assert_eq!(graph.edges[0].source, "Engine");
        assert_eq!(graph.edges[0].target, "PowerSource");
    }

    #[test]
    fn bdd_composition_edge() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Engine;
            part def Vehicle {
                part engine : Engine;
            }
        "#,
        );
        let graph = build_bdd(&model, None);
        let comp_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Composition)
            .collect();
        assert_eq!(comp_edges.len(), 1);
        assert_eq!(comp_edges[0].source, "Vehicle");
        assert_eq!(comp_edges[0].target, "Engine");
        assert_eq!(comp_edges[0].label.as_deref(), Some("engine"));
    }

    #[test]
    fn bdd_scoped() {
        let model = parse_file(
            "test.sysml",
            r#"
            package Pkg {
                part def Vehicle {
                    part def Engine;
                }
                part def Unrelated;
            }
        "#,
        );
        let graph = build_bdd(&model, Some("Vehicle"));
        assert!(graph.has_node("Vehicle"));
        assert!(graph.has_node("Engine"));
        assert!(!graph.has_node("Unrelated"));
    }

    #[test]
    fn ibd_parts_and_ports() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle {
                part engine : Engine;
                part transmission : Transmission;
                port fuelIn : FuelPort;
            }
        "#,
        );
        let graph = build_ibd(&model, "Vehicle");
        assert_eq!(graph.kind, DiagramKind::Ibd);
        assert!(graph.has_node("engine"));
        assert!(graph.has_node("transmission"));
        assert!(graph.has_node("fuelIn"));
        let port_nodes: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Port)
            .collect();
        assert_eq!(port_nodes.len(), 1);
    }

    #[test]
    fn req_diagram_with_satisfy() {
        let model = parse_file(
            "test.sysml",
            r#"
            requirement def MassReq {
                doc /* mass < 2000 */
            }
            part def Vehicle {
                satisfy MassReq;
            }
        "#,
        );
        let graph = build_req(&model);
        assert!(graph.has_node("MassReq"));
        let sat_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Satisfy)
            .collect();
        assert!(!sat_edges.is_empty());
    }

    #[test]
    fn pkg_diagram_with_package() {
        let model = parse_file(
            "test.sysml",
            r#"
            package VehicleModel {
                part def Vehicle;
                part def Engine;
            }
        "#,
        );
        let graph = build_pkg(&model);
        assert!(graph.has_node("VehicleModel"));
        assert!(graph.has_node("Vehicle"));
        assert!(!graph.subgraphs.is_empty());
        assert_eq!(graph.subgraphs[0].node_ids.len(), 2);
    }

    #[test]
    fn par_diagram_with_constraints() {
        let model = parse_file(
            "test.sysml",
            r#"
            constraint def MassConstraint {
                in massActual : Real;
                in massLimit : Real;
                massActual <= massLimit;
            }
        "#,
        );
        let graph = build_par(&model, None);
        assert!(graph.has_node("MassConstraint"));
        let node = graph.nodes.iter().find(|n| n.id == "MassConstraint").unwrap();
        assert_eq!(node.kind, NodeKind::Constraint);
    }

    #[test]
    fn rich_stm_has_transition_labels() {
        use crate::sim::state_machine::*;
        use crate::model::Span;

        let sm = StateMachineModel {
            name: "Traffic".to_string(),
            states: vec![
                StateNode {
                    name: "Red".to_string(),
                    entry_action: Some(ActionRef::Named("stopTraffic".to_string())),
                    do_action: None,
                    exit_action: None,
                    span: Span::default(),
                },
                StateNode {
                    name: "Green".to_string(),
                    entry_action: None,
                    do_action: None,
                    exit_action: None,
                    span: Span::default(),
                },
            ],
            transitions: vec![Transition {
                name: None,
                source: "Red".to_string(),
                target: "Green".to_string(),
                trigger: Some(Trigger::Signal("timer".to_string())),
                guard: None,
                effect: Some(ActionRef::Named("startGo".to_string())),
                span: Span::default(),
            }],
            entry_state: Some("Red".to_string()),
            span: Span::default(),
        };

        let graph = build_stm_from_state_machine(&sm);
        assert_eq!(graph.kind, DiagramKind::Stm);
        assert!(graph.has_node("Red"));
        assert!(graph.has_node("Green"));
        assert!(graph.has_node("__initial__"));

        // Check transition label includes trigger and effect
        let tr = graph.edges.iter().find(|e| e.source == "Red").unwrap();
        let label = tr.label.as_deref().unwrap();
        assert!(label.contains("timer"));
        assert!(label.contains("/ startGo"));

        // Check entry action on Red state
        let red = graph.nodes.iter().find(|n| n.id == "Red").unwrap();
        assert!(red.attributes.iter().any(|(k, _)| k == "entry"));
    }

    #[test]
    fn act_from_action_model() {
        use crate::sim::action_flow::*;
        use crate::model::Span;

        let am = ActionModel {
            name: "Process".to_string(),
            steps: vec![
                ActionStep::Perform {
                    name: "Step1".to_string(),
                    span: Span::default(),
                },
                ActionStep::Perform {
                    name: "Step2".to_string(),
                    span: Span::default(),
                },
                ActionStep::Done { span: Span::default() },
            ],
            span: Span::default(),
        };

        let graph = build_act_from_action_model(&am);
        assert_eq!(graph.kind, DiagramKind::Act);
        assert!(graph.has_node("__initial__"));
        assert!(graph.has_node("__final__"));

        // Should have action nodes
        let actions: Vec<_> = graph.nodes.iter()
            .filter(|n| n.kind == NodeKind::Action)
            .collect();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].label, "Step1");
        assert_eq!(actions[1].label, "Step2");

        // Flow edges: initial->Step1, Step1->Step2, Step2->final
        assert!(graph.edges.len() >= 3);
    }

    #[test]
    fn act_with_decision() {
        use crate::sim::action_flow::*;
        use crate::sim::expr::Expr;
        use crate::model::Span;

        let am = ActionModel {
            name: "Branching".to_string(),
            steps: vec![
                ActionStep::IfAction {
                    condition: Expr::Var("ready".to_string()),
                    then_step: Box::new(ActionStep::Perform {
                        name: "Go".to_string(),
                        span: Span::default(),
                    }),
                    else_step: Some(Box::new(ActionStep::Perform {
                        name: "Wait".to_string(),
                        span: Span::default(),
                    })),
                    span: Span::default(),
                },
            ],
            span: Span::default(),
        };

        let graph = build_act_from_action_model(&am);
        // Should have decision node
        let decisions: Vec<_> = graph.nodes.iter()
            .filter(|n| n.kind == NodeKind::Decision)
            .collect();
        assert!(!decisions.is_empty());
        assert!(graph.has_node("__final__"));
    }
}
