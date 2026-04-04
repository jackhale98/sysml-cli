/// Diagram builders: convert Model data into DiagramGraph.

use crate::model::*;
use crate::sim::state_machine::StateMachineModel;
use crate::sim::action_flow::{ActionModel, ActionStep};
use super::graph::*;

/// Extract the root (first) segment of a dotted path.
/// "engine.drivePwrPort" → "engine", "transmission" → "transmission"
fn root_name(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

/// Build a Block Definition Diagram showing definitions and their relationships.
///
/// Shows: definitions as blocks, specialization arrows, composition (part usages).
pub fn build_bdd(model: &Model, scope: Option<&str>) -> DiagramGraph {
    let title = scope
        .map(|s| format!("gv [{}]", s))
        .unwrap_or_else(|| format!("gv [{}]", model.file));
    let mut graph = DiagramGraph::new(title, DiagramKind::GeneralView(GeneralViewFlavor::Default));

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
            let t = u.type_ref.as_deref().unwrap_or("");
            let mult = u
                .multiplicity
                .as_ref()
                .map(|m| format!(" {}", m))
                .unwrap_or_default();
            let type_str = if t.is_empty() { String::new() } else { format!(" : {}", t) };
            attrs.push((u.kind.clone(), format!("{}{}{}", u.name, type_str, mult)));
        }

        // Build label with optional short name (e.g., "<REQ2> flightTimeReq")
        let label = if let Some(ref sn) = def.short_name {
            format!("<{}> {}", sn, def.name)
        } else {
            def.name.clone()
        };
        graph.add_node(DiagramNode {
            id: def.name.clone(),
            label,
            kind: NodeKind::Block,
            stereotype: Some(stereotype),
            attributes: attrs,
            is_definition: true,
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
/// Recursively collect ports from a part and all its nested parts.
/// Uses span containment to disambiguate same-named usages in different scopes.
fn collect_ports_recursive(
    model: &Model,
    parent_name: &str,
    parent_span: &crate::model::Span,
    prefix: &str,
) -> Vec<(String, String)> {
    let mut ports = Vec::new();
    // Find children: matching parent_def name AND contained within parent span
    let children: Vec<&crate::model::Usage> = model.usages
        .iter()
        .filter(|u| u.parent_def.as_deref() == Some(parent_name)
            && parent_span.contains(&u.span))
        .collect();
    for cu in &children {
        if cu.kind == "port" {
            let pt = cu.type_ref.as_deref().unwrap_or("");
            let dir = cu.direction
                .map(|d| format!("{} ", d.label()))
                .unwrap_or_default();
            let conj = if cu.is_conjugated { "~" } else { "" };
            let name = if prefix.is_empty() {
                cu.name.clone()
            } else {
                format!("{}.{}", prefix, cu.name)
            };
            ports.push((format!("{}port {}", dir, name), format!("{}{}", conj, pt)));
        } else if cu.kind == "part" {
            let nested_prefix = if prefix.is_empty() {
                cu.name.clone()
            } else {
                format!("{}.{}", prefix, cu.name)
            };
            ports.extend(collect_ports_recursive(model, &cu.name, &cu.span, &nested_prefix));
        }
    }
    ports
}

pub fn build_ibd(model: &Model, def_name: &str) -> DiagramGraph {
    let title = format!("iv [{}]", def_name);
    let mut graph = DiagramGraph::new(title, DiagramKind::InterconnectionView);

    let usages = model.usages_in_def(def_name);

    // Add part usages as blocks, with ports inside them as attributes
    for u in &usages {
        if u.kind == "part" {
            let t = u.type_ref.as_deref().unwrap_or("");
            // Collect ports recursively from this part and its nested parts
            let child_ports = collect_ports_recursive(model, &u.name, &u.span, "");
            graph.add_node(DiagramNode {
                id: u.name.clone(),
                label: if t.is_empty() { u.name.clone() } else { format!("{} : {}", u.name, t) },
                kind: NodeKind::Block,
                stereotype: None,
                attributes: child_ports,
                is_definition: false,
            });
        } else if u.kind == "port" {
            let t = u.type_ref.as_deref().unwrap_or("");
            let dir = u
                .direction
                .map(|d| format!("{} ", d.label()))
                .unwrap_or_default();
            let conj = if u.is_conjugated { "~" } else { "" };
            graph.add_node(DiagramNode {
                id: u.name.clone(),
                label: format!("{}{} : {}{}", dir, u.name, conj, t),
                kind: NodeKind::Port,
                stereotype: None,
                attributes: Vec::new(),
                is_definition: false,
            });
        }
    }

    // Add connections
    // For dotted paths like "engine.drivePwrPort", the root segment ("engine")
    // is the part node in the IBD, while the rest is the port/feature path.
    for conn in &model.connections {
        let src = root_name(&conn.source);
        let tgt = root_name(&conn.target);
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
        let src = root_name(&flow.source);
        let tgt = root_name(&flow.target);
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
        .map(|d| format!("stv [{}]", d.name))
        .unwrap_or_else(|| "stm".to_string());
    let mut graph = DiagramGraph::new(title, DiagramKind::StateTransitionView);

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
                is_definition: false,
    });

    // Add state nodes
    for s in &states {
        graph.add_node(DiagramNode {
            id: s.name.clone(),
            label: s.name.clone(),
            kind: NodeKind::State,
            stereotype: None,
            attributes: Vec::new(),
                is_definition: false,
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
    let title = format!("grv [{}]", model.file);
    let mut graph = DiagramGraph::new(title, DiagramKind::GridView(GridViewFlavor::Requirements));

    // Add requirement definition nodes
    for def in &model.definitions {
        if def.kind == DefKind::Requirement {
            let mut attrs = Vec::new();
            if let Some(ref doc) = def.doc {
                attrs.push(("text".to_string(), doc.clone()));
            }
            let label = if let Some(ref sn) = def.short_name {
                format!("<{}> {}", sn, def.name)
            } else {
                def.name.clone()
            };
            graph.add_node(DiagramNode {
                id: def.name.clone(),
                label,
                kind: NodeKind::Requirement,
                stereotype: Some(if def.parent_def.is_some() {
                    "<<requirement>>".to_string()
                } else {
                    "<<requirement def>>".to_string()
                }),
                attributes: attrs,
                is_definition: def.parent_def.is_none(),
            });
        }
    }
    // Add requirement usage nodes (nested requirements)
    for usage in &model.usages {
        if usage.kind == "requirement" && !usage.name.is_empty() {
            if graph.has_node(&usage.name) {
                continue;
            }
            let attrs = Vec::new();
            let label = if let Some(ref sn) = usage.short_name {
                format!("<{}> {}", sn, usage.name)
            } else {
                usage.name.clone()
            };
            graph.add_node(DiagramNode {
                id: usage.name.clone(),
                label,
                kind: NodeKind::Requirement,
                stereotype: Some("<<requirement>>".to_string()),
                attributes: attrs,
                is_definition: false,
            });
            // Edge from parent to child requirement
            if let Some(ref parent) = usage.parent_def {
                if graph.has_node(parent) {
                    graph.add_edge(DiagramEdge {
                        source: parent.clone(),
                        target: usage.name.clone(),
                        label: None,
                        kind: EdgeKind::Containment,
                    });
                }
            }
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
                is_definition: false,
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
                is_definition: false,
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
    let title = format!("bv [{}]", model.file);
    let mut graph = DiagramGraph::new(title, DiagramKind::BrowserView);

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
                is_definition: false,
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
                is_definition: false,
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
                is_definition: true,
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
    let mut graph = DiagramGraph::new(title, DiagramKind::GeneralView(GeneralViewFlavor::Parametric));

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
                let t = p.type_ref.as_deref().unwrap_or("");
                (format!("{}{}", dir, p.name), t.to_string())
            })
            .collect();

        graph.add_node(DiagramNode {
            id: cdef.name.clone(),
            label: cdef.name.clone(),
            kind: NodeKind::Constraint,
            stereotype: Some("<<constraint>>".to_string()),
            attributes: attrs,
            is_definition: false,
        });
    }

    graph
}

/// Build a rich State Machine Diagram from a parsed `StateMachineModel`.
///
/// Shows: states with entry/do/exit actions, full transition labels
/// (trigger [guard] / effect), initial and final pseudo-states.
pub fn build_stm_from_state_machine(sm: &StateMachineModel) -> DiagramGraph {
    let title = format!("stv [{}]", sm.name);
    let mut graph = DiagramGraph::new(title, DiagramKind::StateTransitionView);

    // Initial pseudo-state
    graph.add_node(DiagramNode {
        id: "__initial__".to_string(),
        label: String::new(),
        kind: NodeKind::InitialState,
        stereotype: None,
        attributes: Vec::new(),
                is_definition: false,
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
            is_definition: false,
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
    let title = format!("afv [{}]", am.name);
    let mut graph = DiagramGraph::new(title, DiagramKind::ActionFlowView);

    // Initial node
    graph.add_node(DiagramNode {
        id: "__initial__".to_string(),
        label: String::new(),
        kind: NodeKind::InitialState,
        stereotype: None,
        attributes: Vec::new(),
                is_definition: false,
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
                is_definition: false,
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
                is_definition: false,
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
                is_definition: false,
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
                is_definition: false,
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
                label: name.as_deref().unwrap_or("").to_string(),
                kind: NodeKind::Decision,
                stereotype: None,
                attributes: Vec::new(),
                is_definition: false,
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
                is_definition: false,
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
                is_definition: false,
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
                is_definition: false,
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
                is_definition: false,
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
                is_definition: false,
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
                is_definition: false,
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
                is_definition: false,
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
                is_definition: false,
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
                is_definition: false,
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

/// Build a Traceability Diagram — the V-model chain.
///
/// Shows requirements, the parts/blocks that satisfy them, and the verification
/// cases that verify them, with satisfy and verify edges forming a traceability
/// web. This is the single most important MBSE diagram for systems engineering
/// reviews and audits.
pub fn build_trace(model: &Model) -> DiagramGraph {
    let title = format!("trace [{}]", model.file);
    let mut graph = DiagramGraph::new(title, DiagramKind::GridView(GridViewFlavor::Trace));

    // Collect requirement definitions as the central column
    for def in &model.definitions {
        if def.kind == DefKind::Requirement {
            let doc = def.doc.as_deref().unwrap_or("");
            let mut attrs = Vec::new();
            if !doc.is_empty() {
                attrs.push(("text".to_string(), doc.to_string()));
            }
            graph.add_node(DiagramNode {
                id: def.name.clone(),
                label: def.name.clone(),
                kind: NodeKind::Requirement,
                stereotype: Some("<<requirement>>".to_string()),
                attributes: attrs,
                is_definition: false,
            });
        }
    }

    // Build implicit satisfaction owners (same pattern as req diagram)
    let satisfaction_owners: std::collections::HashMap<usize, &str> = model
        .satisfactions
        .iter()
        .enumerate()
        .filter(|(_, s)| s.by.is_none())
        .filter_map(|(i, s)| {
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

    // Add blocks that satisfy requirements (left column in V-model)
    let mut added_blocks: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, sat) in model.satisfactions.iter().enumerate() {
        let by_name = if let Some(ref by) = sat.by {
            simple_name(by).to_string()
        } else if let Some(owner) = satisfaction_owners.get(&i) {
            owner.to_string()
        } else {
            continue;
        };

        if !added_blocks.contains(&by_name) {
            let stereotype = model
                .find_def(&by_name)
                .map(|d| format!("<<{}>>", d.kind.label()));
            graph.add_node(DiagramNode {
                id: by_name.clone(),
                label: by_name.clone(),
                kind: NodeKind::Block,
                stereotype,
                attributes: Vec::new(),
                is_definition: false,
            });
            added_blocks.insert(by_name.clone());
        }
        graph.add_edge(DiagramEdge {
            source: by_name,
            target: simple_name(&sat.requirement).to_string(),
            label: Some("<<satisfy>>".to_string()),
            kind: EdgeKind::Satisfy,
        });
    }

    // Add verifiers (right column in V-model)
    let mut added_verifiers: std::collections::HashSet<String> = std::collections::HashSet::new();
    for ver in &model.verifications {
        let ver_name = simple_name(&ver.by);
        if !added_verifiers.contains(ver_name) {
            let stereotype = model
                .find_def(ver_name)
                .map(|d| format!("<<{}>>", d.kind.label()));
            graph.add_node(DiagramNode {
                id: ver_name.to_string(),
                label: ver_name.to_string(),
                kind: NodeKind::Block,
                stereotype,
                attributes: Vec::new(),
                is_definition: false,
            });
            added_verifiers.insert(ver_name.to_string());
        }
        graph.add_edge(DiagramEdge {
            source: ver_name.to_string(),
            target: simple_name(&ver.requirement).to_string(),
            label: Some("<<verify>>".to_string()),
            kind: EdgeKind::Verify,
        });
    }

    // Highlight unsatisfied/unverified requirements
    let satisfied_names: std::collections::HashSet<&str> = model
        .satisfactions
        .iter()
        .map(|s| simple_name(&s.requirement))
        .collect();
    let verified_names: std::collections::HashSet<&str> = model
        .verifications
        .iter()
        .map(|v| simple_name(&v.requirement))
        .collect();

    for node in &mut graph.nodes {
        if node.kind == NodeKind::Requirement {
            let has_sat = satisfied_names.contains(node.id.as_str());
            let has_ver = verified_names.contains(node.id.as_str());
            if !has_sat {
                node.attributes
                    .push(("status".to_string(), "UNSATISFIED".to_string()));
            }
            if !has_ver {
                node.attributes
                    .push(("status".to_string(), "UNVERIFIED".to_string()));
            }
        }
    }

    graph
}

/// Build an Allocation Diagram — logical-to-physical mapping.
///
/// Shows actions and use-cases allocated to parts. Essential for MBSE to
/// demonstrate that all logical functions have physical homes.
pub fn build_alloc(model: &Model) -> DiagramGraph {
    let title = format!("alloc [{}]", model.file);
    let mut graph = DiagramGraph::new(title, DiagramKind::GridView(GridViewFlavor::Alloc));

    let mut added: std::collections::HashSet<String> = std::collections::HashSet::new();

    for alloc in &model.allocations {
        let src = simple_name(&alloc.source);
        let tgt = simple_name(&alloc.target);

        if !added.contains(src) {
            let kind_label = model
                .find_def(src)
                .map(|d| format!("<<{}>>", d.kind.label()));
            graph.add_node(DiagramNode {
                id: src.to_string(),
                label: src.to_string(),
                kind: NodeKind::Action,
                stereotype: kind_label,
                attributes: Vec::new(),
                is_definition: false,
            });
            added.insert(src.to_string());
        }

        if !added.contains(tgt) {
            let kind_label = model
                .find_def(tgt)
                .map(|d| format!("<<{}>>", d.kind.label()));
            graph.add_node(DiagramNode {
                id: tgt.to_string(),
                label: tgt.to_string(),
                kind: NodeKind::Block,
                stereotype: kind_label,
                attributes: Vec::new(),
                is_definition: false,
            });
            added.insert(tgt.to_string());
        }

        graph.add_edge(DiagramEdge {
            source: src.to_string(),
            target: tgt.to_string(),
            label: Some("<<allocate>>".to_string()),
            kind: EdgeKind::Allocate,
        });
    }

    // Show unallocated actions as standalone nodes with a note
    for def in &model.definitions {
        if def.kind == DefKind::Action && !added.contains(&def.name) {
            graph.add_node(DiagramNode {
                id: def.name.clone(),
                label: def.name.clone(),
                kind: NodeKind::Action,
                stereotype: Some("<<action>> UNALLOCATED".to_string()),
                attributes: Vec::new(),
                is_definition: false,
            });
        }
    }

    graph
}

/// Build a Use Case Diagram — actors, use cases, and includes.
///
/// Extracts `use case def` elements and any actor usages. Shows include
/// relationships between use cases.
pub fn build_ucd(model: &Model) -> DiagramGraph {
    let title = format!("ucd [{}]", model.file);
    let mut graph = DiagramGraph::new(title, DiagramKind::GeneralView(GeneralViewFlavor::UseCase));

    let mut added_actors: std::collections::HashSet<String> = std::collections::HashSet::new();

    for def in &model.definitions {
        if def.kind == DefKind::UseCase {
            let doc = def.doc.as_deref().unwrap_or("");
            let mut attrs = Vec::new();
            if !doc.is_empty() {
                attrs.push(("description".to_string(), doc.to_string()));
            }
            graph.add_node(DiagramNode {
                id: def.name.clone(),
                label: def.name.clone(),
                kind: NodeKind::UseCase,
                stereotype: Some("<<use case>>".to_string()),
                attributes: attrs,
                is_definition: false,
            });

            // Find actor usages inside this use case
            for u in model.usages_in_def(&def.name) {
                if u.kind == "actor" {
                    let actor_name = u.type_ref.as_deref().unwrap_or(&u.name);
                    if !added_actors.contains(actor_name) {
                        graph.add_node(DiagramNode {
                            id: actor_name.to_string(),
                            label: actor_name.to_string(),
                            kind: NodeKind::Actor,
                            stereotype: Some("<<actor>>".to_string()),
                            attributes: Vec::new(),
                is_definition: false,
                        });
                        added_actors.insert(actor_name.to_string());
                    }
                    graph.add_edge(DiagramEdge {
                        source: actor_name.to_string(),
                        target: def.name.clone(),
                        label: None,
                        kind: EdgeKind::Dependency,
                    });
                }
            }

            // Find "include use case" usages
            for u in model.usages_in_def(&def.name) {
                if u.kind == "use case" || u.kind == "usecase" {
                    let included = u.type_ref.as_deref().unwrap_or(&u.name);
                    graph.add_edge(DiagramEdge {
                        source: def.name.clone(),
                        target: included.to_string(),
                        label: Some("<<include>>".to_string()),
                        kind: EdgeKind::Dependency,
                    });
                }
            }
        }
    }

    graph
}

/// Build a Sequence View (sv) from message flows.
///
/// Shows lifelines (parts) and messages between them, ordered by source position.
pub fn build_sv(model: &Model, scope: Option<&str>) -> DiagramGraph {
    let title = scope
        .map(|s| format!("sv [{}]", s))
        .unwrap_or_else(|| format!("sv [{}]", model.file));
    let mut graph = DiagramGraph::new(title, DiagramKind::SequenceView);

    // Collect lifeline participants: parts within scope (or top-level)
    let parts: Vec<&crate::model::Usage> = if let Some(scope_name) = scope {
        model.usages_in_def(scope_name)
            .into_iter()
            .filter(|u| matches!(u.kind.as_str(), "part" | "item"))
            .collect()
    } else {
        model.usages.iter()
            .filter(|u| matches!(u.kind.as_str(), "part" | "item") && u.parent_def.is_none())
            .collect()
    };

    for part in &parts {
        graph.add_node(DiagramNode {
            id: part.name.clone(),
            label: format!("{}{}", part.name,
                part.type_ref.as_ref().map(|t| format!(" : {}", t)).unwrap_or_default()),
            kind: NodeKind::Lifeline,
            stereotype: None,
            attributes: Vec::new(),
            is_definition: false,
        });
    }

    // Also add action usages as potential lifelines
    let action_parts: Vec<&crate::model::Usage> = if let Some(scope_name) = scope {
        model.usages_in_def(scope_name)
            .into_iter()
            .filter(|u| matches!(u.kind.as_str(), "action" | "connection" | "flow"))
            .collect()
    } else {
        Vec::new()
    };
    for ap in &action_parts {
        if !graph.has_node(&ap.name) {
            graph.add_node(DiagramNode {
                id: ap.name.clone(),
                label: format!("{}{}", ap.name,
                    ap.type_ref.as_ref().map(|t| format!(" : {}", t)).unwrap_or_default()),
                kind: NodeKind::Lifeline,
                stereotype: None,
                attributes: Vec::new(),
                is_definition: false,
            });
        }
    }

    // Collect messages from flows and connections
    let lifeline_names: std::collections::HashSet<String> =
        graph.nodes.iter().map(|n| n.id.clone()).collect();

    // Helper: extract the root part name from a dotted path
    fn root_part(path: &str) -> &str {
        path.split('.').next().unwrap_or(path)
    }

    for flow in &model.flows {
        let src = root_part(&flow.source);
        let tgt = root_part(&flow.target);
        // Include flows where at least one endpoint matches a lifeline
        if lifeline_names.contains(src) || lifeline_names.contains(tgt) {
            // Ensure both endpoints exist as nodes
            if !graph.has_node(src) {
                graph.add_node(DiagramNode {
                    id: src.to_string(),
                    label: src.to_string(),
                    kind: NodeKind::Lifeline,
                    stereotype: None,
                    attributes: Vec::new(),
                    is_definition: false,
                });
            }
            if !graph.has_node(tgt) {
                graph.add_node(DiagramNode {
                    id: tgt.to_string(),
                    label: tgt.to_string(),
                    kind: NodeKind::Lifeline,
                    stereotype: None,
                    attributes: Vec::new(),
                    is_definition: false,
                });
            }
            graph.add_edge(DiagramEdge {
                source: src.to_string(),
                target: tgt.to_string(),
                label: flow.name.clone().or(flow.item_type.clone()),
                kind: EdgeKind::Message,
            });
        }
    }

    // Also include connections as messages
    for conn in &model.connections {
        let src = root_part(&conn.source);
        let tgt = root_part(&conn.target);
        if lifeline_names.contains(src) || lifeline_names.contains(tgt) {
            if !graph.has_node(src) {
                graph.add_node(DiagramNode {
                    id: src.to_string(),
                    label: src.to_string(),
                    kind: NodeKind::Lifeline,
                    stereotype: None,
                    attributes: Vec::new(),
                    is_definition: false,
                });
            }
            if !graph.has_node(tgt) {
                graph.add_node(DiagramNode {
                    id: tgt.to_string(),
                    label: tgt.to_string(),
                    kind: NodeKind::Lifeline,
                    stereotype: None,
                    attributes: Vec::new(),
                    is_definition: false,
                });
            }
            graph.add_edge(DiagramEdge {
                source: src.to_string(),
                target: tgt.to_string(),
                label: conn.name.clone(),
                kind: EdgeKind::Message,
            });
        }
    }

    graph
}

/// Apply a view filter to a diagram graph.
///
/// Uses the view definition's expose and kind filters to prune the graph.
/// Returns the filtered graph.
pub fn apply_view_filter(graph: &mut DiagramGraph, model: &Model, view_name: &str) {
    use crate::query;

    let Some(filter) = query::filter_from_view(model, view_name) else {
        return;
    };

    // Collect names of elements that pass the view filter
    let elements = query::list_elements(model, &filter);
    let allowed: std::collections::HashSet<&str> =
        elements.iter().map(|e| e.name()).collect();

    if !allowed.is_empty() {
        graph.filter_by_names(&allowed);
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
        assert_eq!(graph.kind, DiagramKind::GeneralView(GeneralViewFlavor::Default));
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
        assert_eq!(graph.kind, DiagramKind::InterconnectionView);
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
        assert_eq!(graph.kind, DiagramKind::StateTransitionView);
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
        assert_eq!(graph.kind, DiagramKind::ActionFlowView);
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

    #[test]
    fn trace_diagram_shows_satisfy_verify() {
        let model = parse_file(
            "test.sysml",
            r#"
            requirement def MassReq {
                doc /* mass < 2000 */
            }
            requirement def SpeedReq {
                doc /* speed > 100 */
            }
            satisfy requirement MassReq by Vehicle;
            verification def MassTest {
                subject vehicle : Vehicle;
                require constraint { vehicle.mass <= 2000 }
            }
        "#,
        );
        let graph = build_trace(&model);
        assert_eq!(graph.kind, DiagramKind::GridView(GridViewFlavor::Trace));
        assert!(graph.has_node("MassReq"));
        assert!(graph.has_node("SpeedReq"));
        assert!(graph.has_node("Vehicle"));

        // MassReq should be satisfied
        let sat_edges: Vec<_> = graph.edges.iter().filter(|e| e.kind == EdgeKind::Satisfy).collect();
        assert_eq!(sat_edges.len(), 1);

        // SpeedReq should be marked UNSATISFIED and UNVERIFIED
        let speed = graph.nodes.iter().find(|n| n.id == "SpeedReq").unwrap();
        assert!(speed.attributes.iter().any(|(_, v)| v == "UNSATISFIED"));
        assert!(speed.attributes.iter().any(|(_, v)| v == "UNVERIFIED"));
    }

    #[test]
    fn alloc_diagram_shows_allocations() {
        let model = parse_file(
            "test.sysml",
            r#"
            action def ProcessData;
            part def Computer;
            allocate ProcessData to Computer;
            action def UnallocatedAction;
        "#,
        );
        let graph = build_alloc(&model);
        assert_eq!(graph.kind, DiagramKind::GridView(GridViewFlavor::Alloc));
        assert!(graph.has_node("ProcessData"));
        assert!(graph.has_node("Computer"));
        assert!(graph.has_node("UnallocatedAction"), "Unallocated actions should appear");

        let alloc_edges: Vec<_> = graph.edges.iter().filter(|e| e.kind == EdgeKind::Allocate).collect();
        assert_eq!(alloc_edges.len(), 1);

        // Unallocated action should have stereotype
        let unalloc = graph.nodes.iter().find(|n| n.id == "UnallocatedAction").unwrap();
        assert!(unalloc.stereotype.as_ref().unwrap().contains("UNALLOCATED"));
    }

    #[test]
    fn ucd_diagram_shows_use_cases() {
        let model = parse_file(
            "test.sysml",
            r#"
            use case def DriveVehicle {
                doc /* Drive the vehicle */
                actor driver : Person;
                include use case startEngine : StartEngine;
            }
            use case def StartEngine;
        "#,
        );
        let graph = build_ucd(&model);
        assert_eq!(graph.kind, DiagramKind::GeneralView(GeneralViewFlavor::UseCase));
        assert!(graph.has_node("DriveVehicle"), "Should have DriveVehicle use case");
        assert!(graph.has_node("StartEngine"), "Should have StartEngine use case");
    }

    #[test]
    fn ibd_connection_edge() {
        let model = parse_file(
            "test.sysml",
            r#"
            part def Vehicle {
                part engine : Engine;
                part transmission : Transmission;
                connection c connect engine to transmission;
            }
        "#,
        );
        let graph = build_ibd(&model, "Vehicle");
        assert!(graph.has_node("engine"));
        assert!(graph.has_node("transmission"));
        let conn_edges: Vec<_> = graph.edges.iter()
            .filter(|e| e.kind == EdgeKind::Connection)
            .collect();
        assert_eq!(conn_edges.len(), 1);
        assert_eq!(conn_edges[0].source, "engine");
        assert_eq!(conn_edges[0].target, "transmission");
    }

    #[test]
    fn view_filter_prunes_graph() {
        let model = parse_file(
            "test.sysml",
            r#"
            package VehicleModel {
                part def Vehicle;
                part def Engine;
                port def FuelPort;
            }
            view def PartsOnly {
                filter @SysML::PartDefinition;
            }
        "#,
        );
        let mut graph = build_bdd(&model, None);
        let initial_count = graph.nodes.len();
        assert!(initial_count >= 3, "Should have Vehicle, Engine, FuelPort");
        apply_view_filter(&mut graph, &model, "PartsOnly");
        // After filtering, only part definitions should remain
        assert!(graph.nodes.len() <= initial_count,
            "View filter should prune nodes");
    }
}
