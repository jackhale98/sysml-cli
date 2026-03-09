/// Action flow model types for simulation.

use crate::model::Span;
use crate::sim::expr::Expr;
use serde::Serialize;

/// An action definition extracted from the model.
#[derive(Debug, Clone, Serialize)]
pub struct ActionModel {
    pub name: String,
    pub steps: Vec<ActionStep>,
    pub span: Span,
}

/// A single step within an action flow.
#[derive(Debug, Clone, Serialize)]
pub enum ActionStep {
    /// Named action reference (perform / inline usage).
    Perform {
        name: String,
        span: Span,
    },
    /// Sequential sub-actions connected by `then`.
    Sequence {
        steps: Vec<ActionStep>,
        span: Span,
    },
    /// Fork (parallel execution).
    Fork {
        name: Option<String>,
        branches: Vec<ActionStep>,
        span: Span,
    },
    /// Join (synchronize parallel branches).
    Join {
        name: Option<String>,
        span: Span,
    },
    /// Decision node.
    Decide {
        name: Option<String>,
        branches: Vec<DecideBranch>,
        span: Span,
    },
    /// Merge node.
    Merge {
        name: Option<String>,
        span: Span,
    },
    /// If/else conditional action.
    IfAction {
        condition: Expr,
        then_step: Box<ActionStep>,
        else_step: Option<Box<ActionStep>>,
        span: Span,
    },
    /// Assignment action: `assign x := expr;`
    Assign {
        target: String,
        value: Expr,
        span: Span,
    },
    /// Send action: `send payload via port to target;`
    Send {
        payload: Option<String>,
        via: Option<String>,
        to: Option<String>,
        span: Span,
    },
    /// While loop action.
    WhileLoop {
        condition: Expr,
        body: Box<ActionStep>,
        span: Span,
    },
    /// For loop action.
    ForLoop {
        variable: String,
        collection: String,
        body: Box<ActionStep>,
        span: Span,
    },
    /// Accept action: `accept Signal` or `accept when condition`.
    Accept {
        signal: Option<String>,
        span: Span,
    },
    /// Terminate action: `terminate` or `terminate name;`
    Terminate {
        target: Option<String>,
        span: Span,
    },
    /// Done / terminal node.
    Done {
        span: Span,
    },
}

/// A branch in a decide node.
#[derive(Debug, Clone, Serialize)]
pub struct DecideBranch {
    pub guard: Option<Expr>,
    pub target: String,
}
