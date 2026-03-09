/// Action flow execution engine.

use crate::sim::action_flow::*;
use crate::sim::eval;
use crate::sim::expr::Env;
use serde::Serialize;

/// Current state of an action flow execution.
#[derive(Debug, Clone, Serialize)]
pub struct ActionExecState {
    pub action_name: String,
    pub step: usize,
    pub env: Env,
    pub trace: Vec<ActionExecStep>,
    pub status: ActionExecStatus,
}

/// A single step in the action execution trace.
#[derive(Debug, Clone, Serialize)]
pub struct ActionExecStep {
    pub step: usize,
    pub kind: String,
    pub description: String,
}

/// Status of the action execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ActionExecStatus {
    Running,
    Completed,
    Error,
    MaxSteps,
}

impl std::fmt::Display for ActionExecStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionExecStatus::Running => write!(f, "running"),
            ActionExecStatus::Completed => write!(f, "completed"),
            ActionExecStatus::Error => write!(f, "error"),
            ActionExecStatus::MaxSteps => write!(f, "max steps reached"),
        }
    }
}

/// Configuration for action execution.
#[derive(Debug, Clone)]
pub struct ActionExecConfig {
    pub max_steps: usize,
    pub initial_env: Env,
}

impl Default for ActionExecConfig {
    fn default() -> Self {
        Self {
            max_steps: 1000,
            initial_env: Env::new(),
        }
    }
}

/// Execute an action flow to completion.
pub fn execute_action(model: &ActionModel, config: &ActionExecConfig) -> ActionExecState {
    let mut state = ActionExecState {
        action_name: model.name.clone(),
        step: 0,
        env: config.initial_env.clone(),
        trace: Vec::new(),
        status: ActionExecStatus::Running,
    };

    for action_step in &model.steps {
        if state.status != ActionExecStatus::Running {
            break;
        }
        if state.step >= config.max_steps {
            state.status = ActionExecStatus::MaxSteps;
            break;
        }
        execute_step(action_step, &mut state, config);
    }

    if state.status == ActionExecStatus::Running {
        state.status = ActionExecStatus::Completed;
    }

    state
}

fn execute_step(
    action_step: &ActionStep,
    state: &mut ActionExecState,
    config: &ActionExecConfig,
) {
    if state.step >= config.max_steps {
        state.status = ActionExecStatus::MaxSteps;
        return;
    }

    match action_step {
        ActionStep::Perform { name, .. } => {
            state.trace.push(ActionExecStep {
                step: state.step,
                kind: "perform".to_string(),
                description: format!("perform {}", name),
            });
            state.step += 1;
        }
        ActionStep::Sequence { steps, .. } => {
            for sub_step in steps {
                if state.status != ActionExecStatus::Running {
                    break;
                }
                execute_step(sub_step, state, config);
            }
        }
        ActionStep::Fork { name, branches, .. } => {
            let label = name.as_deref().unwrap_or("fork");
            state.trace.push(ActionExecStep {
                step: state.step,
                kind: "fork".to_string(),
                description: format!("fork {} ({} branches)", label, branches.len()),
            });
            state.step += 1;

            // Execute all branches (simulated sequentially)
            for branch in branches {
                if state.status != ActionExecStatus::Running {
                    break;
                }
                execute_step(branch, state, config);
            }
        }
        ActionStep::Join { name, .. } => {
            let label = name.as_deref().unwrap_or("join");
            state.trace.push(ActionExecStep {
                step: state.step,
                kind: "join".to_string(),
                description: format!("join {}", label),
            });
            state.step += 1;
        }
        ActionStep::Decide { name, branches, .. } => {
            let label = name.as_deref().unwrap_or("decide");
            // Evaluate guards and pick the first matching branch
            let mut taken = None;
            for branch in branches {
                let guard_ok = match &branch.guard {
                    None => true,
                    Some(expr) => eval::evaluate_constraint(expr, &state.env).unwrap_or(false),
                };
                if guard_ok {
                    taken = Some(&branch.target);
                    break;
                }
            }
            let target = taken
                .cloned()
                .unwrap_or_else(|| "none".to_string());
            state.trace.push(ActionExecStep {
                step: state.step,
                kind: "decide".to_string(),
                description: format!("decide {} -> {}", label, target),
            });
            state.step += 1;
        }
        ActionStep::Merge { name, .. } => {
            let label = name.as_deref().unwrap_or("merge");
            state.trace.push(ActionExecStep {
                step: state.step,
                kind: "merge".to_string(),
                description: format!("merge {}", label),
            });
            state.step += 1;
        }
        ActionStep::IfAction {
            condition,
            then_step,
            else_step,
            ..
        } => {
            let cond_result = eval::evaluate_constraint(condition, &state.env).unwrap_or(false);
            state.trace.push(ActionExecStep {
                step: state.step,
                kind: "if".to_string(),
                description: format!("if -> {}", cond_result),
            });
            state.step += 1;

            if cond_result {
                execute_step(then_step, state, config);
            } else if let Some(else_s) = else_step {
                execute_step(else_s, state, config);
            }
        }
        ActionStep::Assign { target, value, .. } => {
            match eval::evaluate(value, &state.env) {
                Ok(val) => {
                    state.trace.push(ActionExecStep {
                        step: state.step,
                        kind: "assign".to_string(),
                        description: format!("assign {} := {}", target, val),
                    });
                    state.env.bind(target, val);
                }
                Err(e) => {
                    state.trace.push(ActionExecStep {
                        step: state.step,
                        kind: "error".to_string(),
                        description: format!("assign {} failed: {}", target, e),
                    });
                    state.status = ActionExecStatus::Error;
                }
            }
            state.step += 1;
        }
        ActionStep::Send {
            payload, via, to, ..
        } => {
            let mut desc = "send".to_string();
            if let Some(p) = payload {
                desc.push_str(&format!(" {}", p));
            }
            if let Some(v) = via {
                desc.push_str(&format!(" via {}", v));
            }
            if let Some(t) = to {
                desc.push_str(&format!(" to {}", t));
            }
            state.trace.push(ActionExecStep {
                step: state.step,
                kind: "send".to_string(),
                description: desc,
            });
            state.step += 1;
        }
        ActionStep::WhileLoop {
            condition, body, ..
        } => {
            let mut iterations = 0;
            loop {
                if state.step >= config.max_steps {
                    state.status = ActionExecStatus::MaxSteps;
                    break;
                }
                let cond_result =
                    eval::evaluate_constraint(condition, &state.env).unwrap_or(false);
                if !cond_result {
                    break;
                }
                state.trace.push(ActionExecStep {
                    step: state.step,
                    kind: "while".to_string(),
                    description: format!("while iteration {}", iterations),
                });
                state.step += 1;
                execute_step(body, state, config);
                iterations += 1;
                if state.status != ActionExecStatus::Running {
                    break;
                }
            }
        }
        ActionStep::ForLoop {
            variable,
            collection,
            body,
            ..
        } => {
            state.trace.push(ActionExecStep {
                step: state.step,
                kind: "for".to_string(),
                description: format!("for {} in {}", variable, collection),
            });
            state.step += 1;
            // Execute body once (no collection runtime available)
            execute_step(body, state, config);
        }
        ActionStep::Accept { signal, .. } => {
            let desc = match signal {
                Some(s) => format!("accept {}", s),
                None => "accept".to_string(),
            };
            state.trace.push(ActionExecStep {
                step: state.step,
                kind: "accept".to_string(),
                description: desc,
            });
            state.step += 1;
        }
        ActionStep::Terminate { target, .. } => {
            let desc = match target {
                Some(t) => format!("terminate {}", t),
                None => "terminate".to_string(),
            };
            state.trace.push(ActionExecStep {
                step: state.step,
                kind: "terminate".to_string(),
                description: desc,
            });
            state.step += 1;
            state.status = ActionExecStatus::Completed;
        }
        ActionStep::Done { .. } => {
            state.trace.push(ActionExecStep {
                step: state.step,
                kind: "done".to_string(),
                description: "done".to_string(),
            });
            state.step += 1;
            state.status = ActionExecStatus::Completed;
        }
    }
}

/// Format action execution trace as human-readable text.
pub fn format_action_trace_text(state: &ActionExecState) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Action: {}", state.action_name));
    lines.push(String::new());

    for step in &state.trace {
        lines.push(format!("  Step {}: [{}] {}", step.step, step.kind, step.description));
    }

    lines.push(String::new());
    lines.push(format!(
        "Status: {} ({} steps)",
        state.status, state.step
    ));

    if !state.env.is_empty() {
        lines.push(String::new());
        lines.push("Environment:".to_string());
        for (k, v) in state.env.iter() {
            lines.push(format!("  {} = {}", k, v));
        }
    }

    lines.join("\n")
}

/// Format action execution trace as JSON.
pub fn format_action_trace_json(state: &ActionExecState) -> String {
    serde_json::to_string_pretty(state).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Span;
    use crate::sim::expr::{BinOp, Expr, Value};

    fn dummy_span() -> Span {
        Span {
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 0,
            start_byte: 0,
            end_byte: 0,
        }
    }

    #[test]
    fn execute_sequential_actions() {
        let model = ActionModel {
            name: "Process".to_string(),
            steps: vec![
                ActionStep::Perform {
                    name: "step1".to_string(),
                    span: dummy_span(),
                },
                ActionStep::Perform {
                    name: "step2".to_string(),
                    span: dummy_span(),
                },
                ActionStep::Perform {
                    name: "step3".to_string(),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };
        let result = execute_action(&model, &ActionExecConfig::default());
        assert_eq!(result.status, ActionExecStatus::Completed);
        assert_eq!(result.step, 3);
        assert_eq!(result.trace.len(), 3);
        assert_eq!(result.trace[0].description, "perform step1");
        assert_eq!(result.trace[1].description, "perform step2");
        assert_eq!(result.trace[2].description, "perform step3");
    }

    #[test]
    fn execute_if_true_branch() {
        let model = ActionModel {
            name: "Conditional".to_string(),
            steps: vec![ActionStep::IfAction {
                condition: Expr::Literal(Value::Bool(true)),
                then_step: Box::new(ActionStep::Perform {
                    name: "yes_action".to_string(),
                    span: dummy_span(),
                }),
                else_step: Some(Box::new(ActionStep::Perform {
                    name: "no_action".to_string(),
                    span: dummy_span(),
                })),
                span: dummy_span(),
            }],
            span: dummy_span(),
        };
        let result = execute_action(&model, &ActionExecConfig::default());
        assert_eq!(result.status, ActionExecStatus::Completed);
        assert_eq!(result.trace.len(), 2); // if + perform
        assert_eq!(result.trace[1].description, "perform yes_action");
    }

    #[test]
    fn execute_if_false_branch() {
        let model = ActionModel {
            name: "Conditional".to_string(),
            steps: vec![ActionStep::IfAction {
                condition: Expr::Literal(Value::Bool(false)),
                then_step: Box::new(ActionStep::Perform {
                    name: "yes_action".to_string(),
                    span: dummy_span(),
                }),
                else_step: Some(Box::new(ActionStep::Perform {
                    name: "no_action".to_string(),
                    span: dummy_span(),
                })),
                span: dummy_span(),
            }],
            span: dummy_span(),
        };
        let result = execute_action(&model, &ActionExecConfig::default());
        assert_eq!(result.trace.len(), 2);
        assert_eq!(result.trace[1].description, "perform no_action");
    }

    #[test]
    fn execute_assign_action() {
        let model = ActionModel {
            name: "Assigner".to_string(),
            steps: vec![
                ActionStep::Assign {
                    target: "x".to_string(),
                    value: Expr::Literal(Value::Number(42.0)),
                    span: dummy_span(),
                },
                ActionStep::Assign {
                    target: "y".to_string(),
                    value: Expr::BinaryOp {
                        op: BinOp::Mul,
                        lhs: Box::new(Expr::Var("x".to_string())),
                        rhs: Box::new(Expr::Literal(Value::Number(2.0))),
                    },
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };
        let result = execute_action(&model, &ActionExecConfig::default());
        assert_eq!(result.status, ActionExecStatus::Completed);
        assert_eq!(result.env.get("x"), Some(&Value::Number(42.0)));
        assert_eq!(result.env.get("y"), Some(&Value::Number(84.0)));
    }

    #[test]
    fn execute_fork_join() {
        let model = ActionModel {
            name: "Parallel".to_string(),
            steps: vec![
                ActionStep::Fork {
                    name: Some("split".to_string()),
                    branches: vec![
                        ActionStep::Perform {
                            name: "branch_a".to_string(),
                            span: dummy_span(),
                        },
                        ActionStep::Perform {
                            name: "branch_b".to_string(),
                            span: dummy_span(),
                        },
                    ],
                    span: dummy_span(),
                },
                ActionStep::Join {
                    name: Some("sync".to_string()),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };
        let result = execute_action(&model, &ActionExecConfig::default());
        assert_eq!(result.status, ActionExecStatus::Completed);
        assert_eq!(result.trace.len(), 4); // fork + 2 branches + join
        assert_eq!(result.trace[0].kind, "fork");
        assert_eq!(result.trace[3].kind, "join");
    }

    #[test]
    fn execute_send_action() {
        let model = ActionModel {
            name: "Sender".to_string(),
            steps: vec![ActionStep::Send {
                payload: Some("SignalA".to_string()),
                via: Some("port1".to_string()),
                to: Some("receiver".to_string()),
                span: dummy_span(),
            }],
            span: dummy_span(),
        };
        let result = execute_action(&model, &ActionExecConfig::default());
        assert_eq!(result.status, ActionExecStatus::Completed);
        assert_eq!(result.trace[0].description, "send SignalA via port1 to receiver");
    }

    #[test]
    fn execute_while_loop() {
        let model = ActionModel {
            name: "Counter".to_string(),
            steps: vec![
                ActionStep::Assign {
                    target: "i".to_string(),
                    value: Expr::Literal(Value::Number(0.0)),
                    span: dummy_span(),
                },
                ActionStep::WhileLoop {
                    condition: Expr::BinaryOp {
                        op: BinOp::Lt,
                        lhs: Box::new(Expr::Var("i".to_string())),
                        rhs: Box::new(Expr::Literal(Value::Number(3.0))),
                    },
                    body: Box::new(ActionStep::Assign {
                        target: "i".to_string(),
                        value: Expr::BinaryOp {
                            op: BinOp::Add,
                            lhs: Box::new(Expr::Var("i".to_string())),
                            rhs: Box::new(Expr::Literal(Value::Number(1.0))),
                        },
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };
        let result = execute_action(&model, &ActionExecConfig::default());
        assert_eq!(result.status, ActionExecStatus::Completed);
        assert_eq!(result.env.get("i"), Some(&Value::Number(3.0)));
    }

    #[test]
    fn execute_max_steps() {
        let model = ActionModel {
            name: "Infinite".to_string(),
            steps: vec![ActionStep::WhileLoop {
                condition: Expr::Literal(Value::Bool(true)),
                body: Box::new(ActionStep::Perform {
                    name: "loop_body".to_string(),
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        };
        let config = ActionExecConfig {
            max_steps: 10,
            initial_env: Env::new(),
        };
        let result = execute_action(&model, &config);
        assert_eq!(result.status, ActionExecStatus::MaxSteps);
    }

    #[test]
    fn format_text_output() {
        let model = ActionModel {
            name: "Test".to_string(),
            steps: vec![ActionStep::Perform {
                name: "doSomething".to_string(),
                span: dummy_span(),
            }],
            span: dummy_span(),
        };
        let result = execute_action(&model, &ActionExecConfig::default());
        let text = format_action_trace_text(&result);
        assert!(text.contains("Action: Test"));
        assert!(text.contains("perform doSomething"));
        assert!(text.contains("completed"));
    }

    #[test]
    fn format_json_output() {
        let model = ActionModel {
            name: "Test".to_string(),
            steps: vec![ActionStep::Perform {
                name: "doSomething".to_string(),
                span: dummy_span(),
            }],
            span: dummy_span(),
        };
        let result = execute_action(&model, &ActionExecConfig::default());
        let json = format_action_trace_json(&result);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_object());
        assert_eq!(parsed["action_name"], "Test");
    }
}
