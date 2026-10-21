//! Hierarchical state machines ("state charts") for interactive emulation.
//!
//! [`state_parser`](crate::sim::state_parser) yields one flat
//! [`StateMachineModel`] per state body. A [`StateChart`] instead keeps a
//! whole machine in one piece: nested states and parallel regions become
//! states with a `parent`, every name is resolved to a unique state path
//! (`flying.positioning.moving`), and initial pseudo-states (`entry; then
//! x;`) and final states (`then done`) are explicit, so a front end can draw
//! the machine and [`state_emulator`](crate::sim::state_emulator) can run it.

use serde::{Serialize, Serializer};
use tree_sitter::{Node, Parser};

use crate::model::Span;
use crate::parser::{get_doc_comment, get_language, node_text};
use crate::sim::expr::Expr;
use crate::sim::state_machine::*;
use crate::sim::state_parser::{extract_state_body, extract_variables};

/// A state machine with its nested states flattened into `parent` links.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateChart {
    pub name: String,
    /// `Package::Owner::name`, when the machine is inside a namespace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    pub file: String,
    /// 1-based line of the declaration.
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// The machine's own top-level states run as parallel regions.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub parallel: bool,
    /// States in document order; parents precede their children.
    pub states: Vec<ChartState>,
    pub transitions: Vec<ChartTransition>,
    /// Attributes of the machine (and of its states, and for an exhibited
    /// state of the owning definition), with their default values.
    pub variables: Vec<StateVariable>,
    /// Names the chart could not resolve, and similar problems.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// What a chart state is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChartStateKind {
    /// Initial pseudo-state of a region (`entry; then x;`).
    Initial,
    State,
    /// Final state of a region (the target of `then done`).
    Final,
}

/// A state of a [`StateChart`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartState {
    /// Unique path of the state in its machine: the simple name for a
    /// top-level state, `outer.inner` for a nested one. An initial
    /// pseudo-state is `entry` (`outer.entry`), a final state `done`.
    pub name: String,
    /// The simple name as declared.
    pub label: String,
    pub kind: ChartStateKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The state's substates are parallel regions.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub parallel: bool,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "action_text"
    )]
    pub entry: Option<ActionRef>,
    #[serde(
        rename = "do",
        skip_serializing_if = "Option::is_none",
        serialize_with = "action_text"
    )]
    pub do_action: Option<ActionRef>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "action_text"
    )]
    pub exit: Option<ActionRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    pub line: usize,
}

/// A transition of a [`StateChart`], between resolved state paths.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartTransition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub from: String,
    pub to: String,
    /// The event that fires it: a signal (`LinkLost`), `when <condition>`
    /// for a change trigger, or a time trigger's text (`after 5 [s]`).
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "trigger_text"
    )]
    pub trigger: Option<Trigger>,
    /// Source text of the guard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guard: Option<String>,
    #[serde(skip)]
    pub guard_expr: Option<Expr>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "action_text"
    )]
    pub effect: Option<ActionRef>,
    pub line: usize,
}

impl ChartTransition {
    /// The transition's name, or `from -> to` when it has none.
    pub fn label(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("{} -> {}", self.from, self.to))
    }
}

fn action_text<S: Serializer>(action: &Option<ActionRef>, s: S) -> Result<S::Ok, S::Error> {
    match action {
        Some(a) => s.serialize_str(&a.to_string()),
        None => s.serialize_none(),
    }
}

fn trigger_text<S: Serializer>(trigger: &Option<Trigger>, s: S) -> Result<S::Ok, S::Error> {
    match trigger {
        Some(t) => s.serialize_str(&t.event_name()),
        None => s.serialize_none(),
    }
}

impl StateChart {
    pub fn state(&self, name: &str) -> Option<&ChartState> {
        self.states.iter().find(|s| s.name == name)
    }

    /// Direct substates of `parent` (`None`: the top-level states).
    pub fn children<'a>(
        &'a self,
        parent: Option<&'a str>,
    ) -> impl Iterator<Item = &'a ChartState> + 'a {
        self.states
            .iter()
            .filter(move |s| s.parent.as_deref() == parent)
    }

    /// Whether `name` has substates.
    pub fn is_composite(&self, name: &str) -> bool {
        self.children(Some(name)).next().is_some()
    }

    /// The parent of a state.
    pub fn parent_of(&self, name: &str) -> Option<&str> {
        self.state(name).and_then(|s| s.parent.as_deref())
    }

    /// `name` and its ancestors, innermost first.
    pub fn ancestry<'a>(&'a self, name: &'a str) -> Vec<&'a str> {
        let mut out = vec![name];
        let mut current = name;
        while let Some(p) = self.parent_of(current) {
            if out.contains(&p) {
                break;
            }
            out.push(p);
            current = p;
        }
        out
    }

    /// Nesting depth (0 for a top-level state).
    pub fn depth(&self, name: &str) -> usize {
        self.ancestry(name).len() - 1
    }
}

/// Extract every state machine in a file as a [`StateChart`]: `state def`s,
/// `exhibit state`s with a body, and state usages with substates that are
/// not nested in another machine.
pub fn extract_state_charts(file: &str, source: &str) -> Vec<StateChart> {
    let mut parser = Parser::new();
    if parser.set_language(&get_language()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut namespace = Vec::new();
    walk(
        tree.root_node(),
        source.as_bytes(),
        file,
        &mut namespace,
        &mut out,
    );
    out
}

fn walk(
    node: Node,
    source: &[u8],
    file: &str,
    namespace: &mut Vec<String>,
    out: &mut Vec<StateChart>,
) {
    match node.kind() {
        "state_definition" | "state_usage" => {
            if let Some(name) = node.child_by_field_name("name") {
                let name = node_text(&name, source).to_string();
                if let Some(chart) = build_chart(&node, name, source, file, namespace, None) {
                    out.push(chart);
                }
            }
            return;
        }
        "exhibit_statement" => {
            let mut cursor = node.walk();
            let name = node
                .children(&mut cursor)
                .find(|c| matches!(c.kind(), "qualified_name" | "identifier"))
                .map(|c| node_text(&c, source).to_string());
            if let Some(name) = name {
                let owner_vars = node
                    .parent()
                    .map(|body| extract_variables(&body, source))
                    .unwrap_or_default();
                if let Some(chart) =
                    build_chart(&node, name, source, file, namespace, Some(owner_vars))
                {
                    out.push(chart);
                }
            }
            return;
        }
        _ => {}
    }
    let pushed = match node.kind() {
        "package_declaration" | "definition" | "usage" => node
            .child_by_field_name("name")
            .map(|n| namespace.push(node_text(&n, source).to_string()))
            .is_some(),
        _ => false,
    };
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, file, namespace, out);
    }
    if pushed {
        namespace.pop();
    }
}

fn is_parallel(node: &Node, source: &[u8]) -> bool {
    let mut c = node.walk();
    let found = node
        .children(&mut c)
        .any(|ch| !ch.is_named() && node_text(&ch, source) == "parallel");
    found
}

fn state_body<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    let mut c = node.walk();
    let found = node
        .children(&mut c)
        .find(|ch| matches!(ch.kind(), "state_body" | "definition_body"));
    found
}

/// A transition before its endpoints are resolved.
struct Pending {
    scope: Option<String>,
    from: Endpoint,
    to: String,
    transition: Transition,
}

enum Endpoint {
    Resolved(String),
    Raw(String),
}

struct Builder<'s> {
    source: &'s [u8],
    states: Vec<ChartState>,
    pending: Vec<Pending>,
    variables: Vec<StateVariable>,
    warnings: Vec<String>,
}

fn path(parent: Option<&str>, name: &str) -> String {
    match parent {
        Some(p) => format!("{p}.{name}"),
        None => name.to_string(),
    }
}

/// 1-based line of a span (span rows are already 1-based).
fn line_of(span: &Span) -> usize {
    span.start_row
}

fn build_chart(
    node: &Node,
    name: String,
    source: &[u8],
    file: &str,
    namespace: &[String],
    owner_variables: Option<Vec<StateVariable>>,
) -> Option<StateChart> {
    let body = state_body(node)?;
    let mut builder = Builder {
        source,
        states: Vec::new(),
        pending: Vec::new(),
        variables: owner_variables.unwrap_or_default(),
        warnings: Vec::new(),
    };
    builder.region(&body, None);
    // A plain `state x { }` with no substates or transitions is a state,
    // not a machine.
    if builder.states.is_empty() && builder.pending.is_empty() {
        return None;
    }
    let transitions = builder.resolve();
    let mut qualified = namespace.to_vec();
    qualified.push(name.clone());
    Some(StateChart {
        qualified_name: (qualified.len() > 1).then(|| qualified.join("::")),
        name,
        file: file.to_string(),
        line: node.start_position().row + 1,
        doc: get_doc_comment(node, source),
        parallel: is_parallel(node, source),
        states: builder.states,
        transitions,
        variables: builder.variables,
        warnings: builder.warnings,
    })
}

impl Builder<'_> {
    /// Add the states and transitions of one state body.
    fn region(&mut self, body: &Node, parent: Option<&str>) {
        let mut nodes = Vec::new();
        let mut transitions = Vec::new();
        let mut entry = None;
        extract_state_body(body, self.source, &mut nodes, &mut transitions, &mut entry);
        for var in extract_variables(body, self.source) {
            if !self.variables.iter().any(|v| v.name == var.name) {
                self.variables.push(var);
            }
        }

        if let Some(target) = entry {
            let initial = path(parent, "entry");
            self.states.push(ChartState {
                name: initial.clone(),
                label: "entry".to_string(),
                kind: ChartStateKind::Initial,
                parent: parent.map(str::to_string),
                parallel: false,
                entry: None,
                do_action: None,
                exit: None,
                doc: None,
                line: line_of(&Span::from_node(body)),
            });
            self.pending.push(Pending {
                scope: parent.map(str::to_string),
                from: Endpoint::Resolved(initial),
                to: target.clone(),
                transition: Transition {
                    name: None,
                    source: String::new(),
                    target,
                    trigger: None,
                    guard: None,
                    guard_text: None,
                    effect: None,
                    span: Span::from_node(body),
                },
            });
        }

        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() != "state_usage" {
                continue;
            }
            let Some(name) = child.child_by_field_name("name") else {
                continue;
            };
            let label = node_text(&name, self.source).to_string();
            let Some(node) = nodes.iter().find(|n| n.name == label) else {
                continue;
            };
            let id = path(parent, &label);
            self.states.push(ChartState {
                name: id.clone(),
                label,
                kind: ChartStateKind::State,
                parent: parent.map(str::to_string),
                parallel: is_parallel(&child, self.source),
                entry: node.entry_action.clone(),
                do_action: node.do_action.clone(),
                exit: node.exit_action.clone(),
                doc: get_doc_comment(&child, self.source),
                line: line_of(&node.span),
            });
            if let Some(inner) = state_body(&child) {
                self.region(&inner, Some(&id));
            }
        }

        for t in transitions {
            self.pending.push(Pending {
                scope: parent.map(str::to_string),
                from: Endpoint::Raw(t.source.clone()),
                to: t.target.clone(),
                transition: t,
            });
        }
    }

    /// Resolve a state reference from inside `scope`: the innermost
    /// enclosing body that declares it wins. `done` is the scope's final
    /// state. Unknown names become placeholder states, with a warning.
    fn resolve_name(&mut self, raw: &str, scope: Option<&str>, context: &str) -> String {
        let raw = raw.replace("::", ".");
        let mut scopes: Vec<Option<String>> = Vec::new();
        let mut current = scope.map(str::to_string);
        loop {
            scopes.push(current.clone());
            match current {
                Some(c) => {
                    current = self
                        .states
                        .iter()
                        .find(|s| s.name == c)
                        .and_then(|s| s.parent.clone());
                }
                None => break,
            }
        }
        for s in &scopes {
            let id = path(s.as_deref(), &raw);
            if self
                .states
                .iter()
                .any(|st| st.name == id && st.kind != ChartStateKind::Initial)
            {
                return id;
            }
        }
        let is_done = raw == "done" || raw.ends_with(".done");
        let id = if is_done {
            path(scope, "done")
        } else {
            path(scope, &raw)
        };
        if !self.states.iter().any(|s| s.name == id) {
            if !is_done {
                self.warnings
                    .push(format!("{context}: unknown state `{raw}`"));
            }
            self.states.push(ChartState {
                name: id.clone(),
                label: if is_done { "done".to_string() } else { raw },
                kind: if is_done {
                    ChartStateKind::Final
                } else {
                    ChartStateKind::State
                },
                parent: scope.map(str::to_string),
                parallel: false,
                entry: None,
                do_action: None,
                exit: None,
                doc: None,
                line: 0,
            });
        }
        id
    }

    fn resolve(&mut self) -> Vec<ChartTransition> {
        let pending = std::mem::take(&mut self.pending);
        let mut out = Vec::new();
        for p in pending {
            let t = p.transition;
            let context = match &t.name {
                Some(n) => format!("transition {n}"),
                None => "transition".to_string(),
            };
            let from = match p.from {
                Endpoint::Resolved(id) => id,
                Endpoint::Raw(raw) => self.resolve_name(&raw, p.scope.as_deref(), &context),
            };
            let to = self.resolve_name(&p.to, p.scope.as_deref(), &context);
            out.push(ChartTransition {
                name: t.name,
                from,
                to,
                trigger: t.trigger,
                guard: t.guard_text,
                guard_expr: t.guard,
                effect: t.effect,
                line: line_of(&t.span),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart(source: &str, name: &str) -> StateChart {
        extract_state_charts("t.sysml", source)
            .into_iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no chart {name}"))
    }

    #[test]
    fn flat_machine_with_variables_and_change_trigger() {
        let c = chart(
            r#"package P {
                state def M {
                    doc /* A machine. */
                    in attribute soc : Real;
                    attribute limit : Real = 0.3;
                    entry; then a;
                    state a { doc /* First. */ }
                    state b { entry action warn; }
                    transition go first a accept Go if soc > 0 then b;
                    transition low first a accept when soc < limit then b;
                    transition back first b then done;
                }
            }"#,
            "M",
        );
        assert_eq!(c.qualified_name.as_deref(), Some("P::M"));
        assert_eq!(c.doc.as_deref(), Some("A machine."));
        let names: Vec<_> = c.states.iter().map(|s| (s.name.as_str(), s.kind)).collect();
        assert_eq!(
            names,
            vec![
                ("entry", ChartStateKind::Initial),
                ("a", ChartStateKind::State),
                ("b", ChartStateKind::State),
                ("done", ChartStateKind::Final),
            ]
        );
        assert_eq!(c.state("a").unwrap().doc.as_deref(), Some("First."));
        assert_eq!(c.transitions[0].from, "entry");
        assert_eq!(c.transitions[0].to, "a");
        let go = &c.transitions[1];
        assert_eq!(go.guard.as_deref(), Some("soc > 0"));
        assert!(matches!(&go.trigger, Some(Trigger::Signal(s)) if s == "Go"));
        let low = &c.transitions[2];
        assert_eq!(
            low.trigger.as_ref().map(Trigger::event_name).as_deref(),
            Some("when soc < limit")
        );
        let vars: Vec<_> = c
            .variables
            .iter()
            .map(|v| (v.name.as_str(), v.value.as_deref(), v.direction.as_deref()))
            .collect();
        assert_eq!(
            vars,
            vec![("soc", None, Some("in")), ("limit", Some("0.3"), None)]
        );
    }

    #[test]
    fn nested_and_parallel_states_get_parents() {
        let c = chart(
            r#"state def Drone {
                entry; then off;
                state off;
                state on parallel {
                    state motors {
                        entry; then idle;
                        state idle;
                        state spinning;
                        transition first idle accept Spin then spinning;
                    }
                    state camera {
                        entry; then idle;
                        state idle;
                    }
                }
                transition first off accept Start then on;
                transition first on accept Stop then off;
            }"#,
            "Drone",
        );
        let on = c.state("on").unwrap();
        assert!(on.parallel);
        assert_eq!(c.state("on.motors").unwrap().parent.as_deref(), Some("on"));
        assert_eq!(
            c.state("on.motors.idle").unwrap().parent.as_deref(),
            Some("on.motors")
        );
        assert!(c.state("on.camera.idle").is_some());
        let spin = c
            .transitions
            .iter()
            .find(|t| t.trigger.as_ref().map(Trigger::event_name).as_deref() == Some("Spin"))
            .unwrap();
        assert_eq!(spin.from, "on.motors.idle");
        assert_eq!(spin.to, "on.motors.spinning");
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.depth("on.motors.idle"), 2);
    }

    #[test]
    fn exhibit_state_sees_owner_attributes() {
        let c = chart(
            r#"part def Lamp {
                attribute power : Real = 5;
                exhibit state modes {
                    entry; then off;
                    state off;
                    state on { entry assign power := power * 2; }
                    transition first off accept Switch then on;
                }
            }"#,
            "modes",
        );
        assert_eq!(c.qualified_name.as_deref(), Some("Lamp::modes"));
        assert_eq!(c.variables[0].name, "power");
        assert_eq!(
            c.state("on").unwrap().entry.as_ref().map(|a| a.to_string()),
            Some("assign power := power * 2".to_string())
        );
    }

    #[test]
    fn unknown_targets_are_reported() {
        let c = chart(
            "state def M { entry; then a; state a; transition first a accept X then nowhere; }",
            "M",
        );
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        assert!(c.state("nowhere").is_some());
    }

    #[test]
    fn leaf_states_are_not_machines() {
        assert!(extract_state_charts("t.sysml", "part def P { state idle; }").is_empty());
    }
}
