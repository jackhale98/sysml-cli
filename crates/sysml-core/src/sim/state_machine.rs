//! State machine model types for simulation.

use crate::model::Span;
use crate::sim::expr::Expr;
use serde::Serialize;

/// A state machine definition extracted from the model.
#[derive(Debug, Clone, Serialize)]
pub struct StateMachineModel {
    pub name: String,
    pub states: Vec<StateNode>,
    pub transitions: Vec<Transition>,
    pub entry_state: Option<String>,
    /// Parallel regions (`state s parallel { state r1 {..} state r2 {..} }`).
    /// Non-empty regions are simulated concurrently with broadcast events.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<StateMachineModel>,
    /// Attributes declared in the machine body (`attribute soc : Real =
    /// 0.3;`). Their defaults seed the simulation environment, and guards
    /// and change triggers read them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<StateVariable>,
    pub span: Span,
}

/// An attribute of a state machine, used as a simulation variable.
#[derive(Debug, Clone, Serialize)]
pub struct StateVariable {
    pub name: String,
    /// Declared type (`Real`, `Boolean`, ...).
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_ref: Option<String>,
    /// Source text of the default value, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The default value as an expression, when the evaluator supports it.
    #[serde(skip)]
    pub expr: Option<Expr>,
    /// `in`, `out` or `inout`, when declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip)]
    pub span: Span,
}

/// A state within a state machine.
#[derive(Debug, Clone, Serialize)]
pub struct StateNode {
    pub name: String,
    pub entry_action: Option<ActionRef>,
    pub do_action: Option<ActionRef>,
    pub exit_action: Option<ActionRef>,
    pub span: Span,
}

/// A transition between states.
#[derive(Debug, Clone, Serialize)]
pub struct Transition {
    pub name: Option<String>,
    pub source: String,
    pub target: String,
    pub trigger: Option<Trigger>,
    pub guard: Option<Expr>,
    /// Source text of the guard. Set whenever the transition has an `if`,
    /// even when `guard` is `None` because the evaluator cannot parse it;
    /// simulators treat such a guard as blocking.
    pub guard_text: Option<String>,
    pub effect: Option<ActionRef>,
    pub span: Span,
}

/// A trigger on a transition.
#[derive(Debug, Clone, Serialize)]
pub enum Trigger {
    /// Signal-based trigger (accept signal).
    Signal(String),
    /// Completion trigger (implicit when do-action finishes).
    Completion,
    /// Change trigger (`accept when <condition>`): fires when the condition
    /// holds. `text` is the source text after `when`; `expr` is `None` when
    /// the evaluator cannot parse the condition.
    Change { text: String, expr: Option<Expr> },
    /// Time trigger (`accept after 5 [s]`, `accept at t`), as source text.
    /// Simulators treat it as an event of that name.
    Time(String),
}

impl Trigger {
    /// The event name a simulator matches against: the signal, `when
    /// <condition>`, or the time trigger's text.
    pub fn event_name(&self) -> String {
        self.to_string()
    }
}

/// One `assign target := value` in an action.
#[derive(Debug, Clone, Serialize)]
pub struct Assignment {
    pub target: String,
    /// Source text of the assigned expression.
    pub value: String,
    /// The value as an expression, when the evaluator supports it.
    #[serde(skip)]
    pub expr: Option<Expr>,
}

/// An action reference (effect, entry, do, exit).
#[derive(Debug, Clone, Serialize)]
pub enum ActionRef {
    /// Reference to a named action.
    Named(String),
    /// Send action.
    Send {
        payload: Option<String>,
        via: Option<String>,
        to: Option<String>,
    },
    /// Inline description.
    Inline(String),
    /// Assignments (`entry assign x := x + 1;`, `do assign ...`, or an
    /// action body of `assign` statements). Simulators apply them to the
    /// environment in order.
    Assign(Vec<Assignment>),
}

impl std::fmt::Display for Trigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Trigger::Signal(s) => write!(f, "{}", s),
            Trigger::Completion => write!(f, "completion"),
            Trigger::Change { text, .. } => write!(f, "when {}", text),
            Trigger::Time(text) => write!(f, "{}", text),
        }
    }
}

impl std::fmt::Display for ActionRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionRef::Named(name) => write!(f, "{}", name),
            ActionRef::Send { payload, via, to } => {
                write!(f, "send")?;
                if let Some(p) = payload {
                    write!(f, " {}", p)?;
                }
                if let Some(v) = via {
                    write!(f, " via {}", v)?;
                }
                if let Some(t) = to {
                    write!(f, " to {}", t)?;
                }
                Ok(())
            }
            ActionRef::Inline(text) => write!(f, "{}", text),
            ActionRef::Assign(assignments) => {
                for (i, a) in assignments.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "assign {} := {}", a.target, a.value)?;
                }
                Ok(())
            }
        }
    }
}
