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
}

impl std::fmt::Display for Trigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Trigger::Signal(s) => write!(f, "{}", s),
            Trigger::Completion => write!(f, "completion"),
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
        }
    }
}
