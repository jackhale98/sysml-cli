//! State machine simulation engine.

use crate::sim::eval;
use crate::sim::expr::Env;
use crate::sim::state_machine::*;
use serde::Serialize;

/// Current state of a simulation run.
#[derive(Debug, Clone, Serialize)]
pub struct SimulationState {
    pub machine_name: String,
    pub current_state: String,
    pub step: usize,
    pub env: Env,
    pub trace: Vec<SimStep>,
    pub status: SimStatus,
}

/// A single step in the simulation trace.
#[derive(Debug, Clone, Serialize)]
pub struct SimStep {
    pub step: usize,
    pub from_state: String,
    pub transition_name: Option<String>,
    pub trigger: Option<String>,
    pub guard_result: Option<bool>,
    pub effect: Option<String>,
    pub to_state: String,
    pub exit_action: Option<String>,
    pub entry_action: Option<String>,
}

/// Status of the simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SimStatus {
    Running,
    Completed,
    Deadlocked,
    MaxSteps,
}

impl std::fmt::Display for SimStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimStatus::Running => write!(f, "running"),
            SimStatus::Completed => write!(f, "completed"),
            SimStatus::Deadlocked => write!(f, "deadlocked"),
            SimStatus::MaxSteps => write!(f, "max steps reached"),
        }
    }
}

/// Configuration for a simulation run.
#[derive(Debug, Clone)]
pub struct SimConfig {
    pub max_steps: usize,
    pub initial_env: Env,
    /// Events to inject in sequence. Each event is consumed when a
    /// matching transition fires.
    pub events: Vec<String>,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            max_steps: 100,
            initial_env: Env::new(),
            events: Vec::new(),
        }
    }
}

/// Run a state machine simulation to completion.
pub fn simulate(machine: &StateMachineModel, config: &SimConfig) -> SimulationState {
    if !machine.regions.is_empty() {
        return simulate_parallel(machine, config);
    }
    let initial = match &machine.entry_state {
        Some(s) => s.clone(),
        None => {
            if let Some(first) = machine.states.first() {
                first.name.clone()
            } else {
                return SimulationState {
                    machine_name: machine.name.clone(),
                    current_state: String::new(),
                    step: 0,
                    env: config.initial_env.clone(),
                    trace: Vec::new(),
                    status: SimStatus::Deadlocked,
                };
            }
        }
    };

    let mut state = SimulationState {
        machine_name: machine.name.clone(),
        current_state: initial,
        step: 0,
        env: config.initial_env.clone(),
        trace: Vec::new(),
        status: SimStatus::Running,
    };

    let mut event_index = 0;

    while state.status == SimStatus::Running {
        if state.step >= config.max_steps {
            state.status = SimStatus::MaxSteps;
            break;
        }

        let current_event = config.events.get(event_index).map(|s| s.as_str());
        let stepped = step(machine, &state, current_event);

        if stepped.status != SimStatus::Running || stepped.trace.len() > state.trace.len() {
            // A transition fired — consume the event if one was used
            if let Some(last_step) = stepped.trace.last() {
                if last_step.trigger.is_some() && current_event.is_some() {
                    event_index += 1;
                }
            }
            state = stepped;
        } else {
            // No transition could fire
            if let Some(evt) = current_event {
                // Record that this event was ignored (no matching transition)
                state.trace.push(SimStep {
                    step: state.step,
                    from_state: state.current_state.clone(),
                    transition_name: None,
                    trigger: Some(evt.to_string()),
                    guard_result: Some(false),
                    effect: None,
                    to_state: state.current_state.clone(),
                    exit_action: None,
                    entry_action: None,
                });
                state.step += 1;
                event_index += 1;
                if event_index >= config.events.len() {
                    // All events consumed, check for auto-transitions
                    let auto_stepped = step(machine, &state, None);
                    if auto_stepped.trace.len() > state.trace.len() {
                        state = auto_stepped;
                    } else {
                        state.status = SimStatus::Completed;
                    }
                }
            } else {
                state.status = if state.trace.is_empty() {
                    SimStatus::Deadlocked
                } else {
                    SimStatus::Completed
                };
            }
        }
    }

    state
}

/// Simulate a parallel state: each region runs concurrently with its own
/// current state; events are broadcast to every region (SysML semantics —
/// an event occurrence is visible to all orthogonal regions). Triggerless
/// (completion) transitions run between events, with per-phase cycle
/// detection so ping-pong loops like `stabilizing <-> moving` settle
/// instead of consuming the whole step budget.
fn simulate_parallel(machine: &StateMachineModel, config: &SimConfig) -> SimulationState {
    let init_region = |r: &StateMachineModel| -> SimulationState {
        SimulationState {
            machine_name: r.name.clone(),
            current_state: r
                .entry_state
                .clone()
                .or_else(|| r.states.first().map(|s| s.name.clone()))
                .unwrap_or_default(),
            step: 0,
            env: config.initial_env.clone(),
            trace: Vec::new(),
            status: SimStatus::Running,
        }
    };

    let mut sims: Vec<SimulationState> = machine.regions.iter().map(init_region).collect();
    let mut trace: Vec<SimStep> = Vec::new();
    let mut step_no = 0usize;
    let mut hit_max = false;

    let record = |region: &str, mut s: SimStep, step_no: &mut usize, trace: &mut Vec<SimStep>| {
        s.step = *step_no;
        s.from_state = format!("{}.{}", region, s.from_state);
        s.to_state = format!("{}.{}", region, s.to_state);
        *step_no += 1;
        trace.push(s);
    };

    // Fire completion (triggerless) transitions per region until each is
    // quiescent or revisits a state within this phase (cycle).
    let run_completion =
        |sims: &mut Vec<SimulationState>, trace: &mut Vec<SimStep>, step_no: &mut usize| -> bool {
            let mut any = false;
            for (i, region) in machine.regions.iter().enumerate() {
                let mut visited: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                visited.insert(sims[i].current_state.clone());
                loop {
                    if *step_no >= config.max_steps {
                        return any;
                    }
                    let next = step(region, &sims[i], None);
                    if next.trace.len() > sims[i].trace.len() {
                        let fired = next.trace.last().unwrap().clone();
                        let target = next.current_state.clone();
                        record(&region.name, fired, step_no, trace);
                        sims[i] = next;
                        any = true;
                        if !visited.insert(target) {
                            break; // cycle within this phase — settle
                        }
                    } else {
                        break;
                    }
                }
            }
            any
        };

    let fired_initially = run_completion(&mut sims, &mut trace, &mut step_no);

    // Broadcast each event to every region, then re-run completions.
    for evt in &config.events {
        if step_no >= config.max_steps {
            hit_max = true;
            break;
        }
        let mut consumed = false;
        for (i, region) in machine.regions.iter().enumerate() {
            let next = step(region, &sims[i], Some(evt));
            if next.trace.len() > sims[i].trace.len() {
                let fired = next.trace.last().unwrap().clone();
                if fired.trigger.is_some() {
                    consumed = true;
                }
                record(&region.name, fired, &mut step_no, &mut trace);
                sims[i] = next;
            }
        }
        if !consumed {
            // Event matched no region — record it as ignored
            let combined = sims
                .iter()
                .map(|s| s.current_state.as_str())
                .collect::<Vec<_>>()
                .join(" | ");
            trace.push(SimStep {
                step: step_no,
                from_state: combined.clone(),
                transition_name: None,
                trigger: Some(evt.clone()),
                guard_result: Some(false),
                effect: None,
                to_state: combined,
                exit_action: None,
                entry_action: None,
            });
            step_no += 1;
        }
        run_completion(&mut sims, &mut trace, &mut step_no);
    }

    let current = sims
        .iter()
        .map(|s| s.current_state.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    let status = if hit_max || step_no >= config.max_steps {
        SimStatus::MaxSteps
    } else if trace.is_empty() && !fired_initially {
        SimStatus::Deadlocked
    } else {
        SimStatus::Completed
    };

    SimulationState {
        machine_name: machine.name.clone(),
        current_state: current,
        step: step_no,
        env: config.initial_env.clone(),
        trace,
        status,
    }
}

/// Advance the simulation by one step.
pub fn step(
    machine: &StateMachineModel,
    state: &SimulationState,
    event: Option<&str>,
) -> SimulationState {
    let mut new_state = state.clone();

    // Find all transitions from the current state
    let candidates: Vec<&Transition> = machine
        .transitions
        .iter()
        .filter(|t| t.source == state.current_state)
        .collect();

    if candidates.is_empty() {
        // Terminal state or done
        if state.current_state == "done" {
            new_state.status = SimStatus::Completed;
        } else {
            new_state.status = SimStatus::Deadlocked;
        }
        return new_state;
    }

    // Try to find an enabled transition
    for transition in &candidates {
        // Check trigger
        let trigger_ok = match &transition.trigger {
            None => true, // No trigger = always available
            Some(Trigger::Completion) => true,
            Some(Trigger::Signal(signal)) => event.is_some_and(|e| e == signal),
        };

        if !trigger_ok {
            continue;
        }

        // Check guard
        let guard_result = match &transition.guard {
            None => true,
            Some(expr) => eval::evaluate_constraint(expr, &state.env).unwrap_or(false),
        };

        if !guard_result {
            continue;
        }

        // Transition fires!
        let from = state.current_state.clone();
        let to = transition.target.clone();

        // Find exit action of source state
        let exit_action = machine
            .states
            .iter()
            .find(|s| s.name == from)
            .and_then(|s| s.exit_action.as_ref())
            .map(|a| a.to_string());

        // Find entry action of target state
        let entry_action = machine
            .states
            .iter()
            .find(|s| s.name == to)
            .and_then(|s| s.entry_action.as_ref())
            .map(|a| a.to_string());

        let effect = transition.effect.as_ref().map(|a| a.to_string());

        let trigger_desc = transition.trigger.as_ref().map(|t| match t {
            Trigger::Signal(s) => s.clone(),
            Trigger::Completion => "completion".to_string(),
        });

        let sim_step = SimStep {
            step: new_state.step,
            from_state: from,
            transition_name: transition.name.clone(),
            trigger: trigger_desc,
            guard_result: transition.guard.as_ref().map(|_| guard_result),
            effect,
            to_state: to.clone(),
            exit_action,
            entry_action,
        };

        new_state.trace.push(sim_step);
        new_state.current_state = to;
        new_state.step += 1;

        if new_state.current_state == "done" {
            new_state.status = SimStatus::Completed;
        }

        return new_state;
    }

    // No transition could fire — return unchanged
    new_state
}

/// Format simulation trace as human-readable text.
pub fn format_trace_text(state: &SimulationState) -> String {
    let mut lines = Vec::new();
    lines.push(format!("State Machine: {}", state.machine_name));
    lines.push(format!(
        "Initial state: {}",
        state
            .trace
            .first()
            .map(|s| s.from_state.as_str())
            .unwrap_or(&state.current_state)
    ));
    lines.push(String::new());

    for step in &state.trace {
        let trigger = step
            .trigger
            .as_deref()
            .map(|t| format!(" [{}]", t))
            .unwrap_or_default();
        let name = step
            .transition_name
            .as_deref()
            .map(|n| format!(" ({})", n))
            .unwrap_or_default();
        let guard = step
            .guard_result
            .map(|g| format!(" guard={}", g))
            .unwrap_or_default();

        lines.push(format!(
            "  Step {}: {} --{}{}{}--> {}",
            step.step, step.from_state, trigger, guard, name, step.to_state,
        ));

        if let Some(ref exit) = step.exit_action {
            lines.push(format!("          exit: {}", exit));
        }
        if let Some(ref effect) = step.effect {
            lines.push(format!("          effect: {}", effect));
        }
        if let Some(ref entry) = step.entry_action {
            lines.push(format!("          entry: {}", entry));
        }
    }

    lines.push(String::new());
    lines.push(format!(
        "Status: {} ({} steps, current: {})",
        state.status, state.step, state.current_state
    ));
    lines.join("\n")
}

/// Format simulation trace as JSON.
pub fn format_trace_json(state: &SimulationState) -> String {
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

    fn traffic_light() -> StateMachineModel {
        StateMachineModel {
            name: "TrafficLight".to_string(),
            states: vec![
                StateNode {
                    name: "red".to_string(),
                    entry_action: None,
                    do_action: None,
                    exit_action: None,
                    span: dummy_span(),
                },
                StateNode {
                    name: "green".to_string(),
                    entry_action: None,
                    do_action: None,
                    exit_action: None,
                    span: dummy_span(),
                },
                StateNode {
                    name: "yellow".to_string(),
                    entry_action: None,
                    do_action: None,
                    exit_action: None,
                    span: dummy_span(),
                },
            ],
            transitions: vec![
                Transition {
                    name: None,
                    source: "red".to_string(),
                    target: "green".to_string(),
                    trigger: Some(Trigger::Signal("next".to_string())),
                    guard: None,
                    effect: None,
                    span: dummy_span(),
                },
                Transition {
                    name: None,
                    source: "green".to_string(),
                    target: "yellow".to_string(),
                    trigger: Some(Trigger::Signal("next".to_string())),
                    guard: None,
                    effect: None,
                    span: dummy_span(),
                },
                Transition {
                    name: None,
                    source: "yellow".to_string(),
                    target: "red".to_string(),
                    trigger: Some(Trigger::Signal("next".to_string())),
                    guard: None,
                    effect: None,
                    span: dummy_span(),
                },
            ],
            entry_state: Some("red".to_string()),
            regions: Vec::new(),
            span: dummy_span(),
        }
    }

    #[test]
    fn simulate_traffic_light_cycle() {
        let machine = traffic_light();
        let config = SimConfig {
            max_steps: 10,
            initial_env: Env::new(),
            events: vec!["next".into(), "next".into(), "next".into()],
        };
        let result = simulate(&machine, &config);
        assert_eq!(result.step, 3);
        assert_eq!(result.current_state, "red"); // Full cycle
        assert_eq!(result.trace.len(), 3);
        assert_eq!(result.trace[0].from_state, "red");
        assert_eq!(result.trace[0].to_state, "green");
        assert_eq!(result.trace[1].to_state, "yellow");
        assert_eq!(result.trace[2].to_state, "red");
    }

    #[test]
    fn simulate_deadlock_no_events() {
        let machine = traffic_light();
        let config = SimConfig {
            max_steps: 10,
            initial_env: Env::new(),
            events: vec![], // No events — all transitions need "next"
        };
        let result = simulate(&machine, &config);
        assert_eq!(result.status, SimStatus::Deadlocked);
        assert_eq!(result.current_state, "red"); // Never moved
    }

    #[test]
    fn simulate_triggerless_transitions() {
        // Transitions without triggers fire immediately
        let machine = StateMachineModel {
            name: "Auto".to_string(),
            states: vec![
                StateNode {
                    name: "a".to_string(),
                    entry_action: None,
                    do_action: None,
                    exit_action: None,
                    span: dummy_span(),
                },
                StateNode {
                    name: "b".to_string(),
                    entry_action: None,
                    do_action: None,
                    exit_action: None,
                    span: dummy_span(),
                },
                StateNode {
                    name: "done".to_string(),
                    entry_action: None,
                    do_action: None,
                    exit_action: None,
                    span: dummy_span(),
                },
            ],
            transitions: vec![
                Transition {
                    name: None,
                    source: "a".to_string(),
                    target: "b".to_string(),
                    trigger: None,
                    guard: None,
                    effect: None,
                    span: dummy_span(),
                },
                Transition {
                    name: None,
                    source: "b".to_string(),
                    target: "done".to_string(),
                    trigger: None,
                    guard: None,
                    effect: None,
                    span: dummy_span(),
                },
            ],
            entry_state: Some("a".to_string()),
            regions: Vec::new(),
            span: dummy_span(),
        };
        let config = SimConfig::default();
        let result = simulate(&machine, &config);
        assert_eq!(result.status, SimStatus::Completed);
        assert_eq!(result.step, 2);
        assert_eq!(result.current_state, "done");
    }

    #[test]
    fn simulate_with_guard() {
        let machine = StateMachineModel {
            name: "Guarded".to_string(),
            states: vec![
                StateNode {
                    name: "idle".to_string(),
                    entry_action: None,
                    do_action: None,
                    exit_action: None,
                    span: dummy_span(),
                },
                StateNode {
                    name: "active".to_string(),
                    entry_action: None,
                    do_action: None,
                    exit_action: None,
                    span: dummy_span(),
                },
            ],
            transitions: vec![Transition {
                name: None,
                source: "idle".to_string(),
                target: "active".to_string(),
                trigger: None,
                guard: Some(Expr::BinaryOp {
                    op: BinOp::Gt,
                    lhs: Box::new(Expr::Var("temperature".to_string())),
                    rhs: Box::new(Expr::Literal(Value::Number(100.0))),
                }),
                effect: None,
                span: dummy_span(),
            }],
            entry_state: Some("idle".to_string()),
            regions: Vec::new(),
            span: dummy_span(),
        };

        // Guard fails — temperature too low
        let mut env_low = Env::new();
        env_low.bind("temperature", Value::Number(50.0));
        let config_low = SimConfig {
            max_steps: 5,
            initial_env: env_low,
            events: vec![],
        };
        let result = simulate(&machine, &config_low);
        assert_eq!(result.status, SimStatus::Deadlocked);
        assert_eq!(result.current_state, "idle");

        // Guard passes — temperature high
        let mut env_high = Env::new();
        env_high.bind("temperature", Value::Number(150.0));
        let config_high = SimConfig {
            max_steps: 5,
            initial_env: env_high,
            events: vec![],
        };
        let result = simulate(&machine, &config_high);
        assert_eq!(result.current_state, "active");
    }

    #[test]
    fn simulate_max_steps() {
        // Self-loop that never ends
        let machine = StateMachineModel {
            name: "Loop".to_string(),
            states: vec![StateNode {
                name: "a".to_string(),
                entry_action: None,
                do_action: None,
                exit_action: None,
                span: dummy_span(),
            }],
            transitions: vec![Transition {
                name: None,
                source: "a".to_string(),
                target: "a".to_string(),
                trigger: None,
                guard: None,
                effect: None,
                span: dummy_span(),
            }],
            entry_state: Some("a".to_string()),
            regions: Vec::new(),
            span: dummy_span(),
        };
        let config = SimConfig {
            max_steps: 5,
            initial_env: Env::new(),
            events: vec![],
        };
        let result = simulate(&machine, &config);
        assert_eq!(result.status, SimStatus::MaxSteps);
        assert_eq!(result.step, 5);
    }

    #[test]
    fn trace_format_text() {
        let machine = traffic_light();
        let config = SimConfig {
            max_steps: 10,
            initial_env: Env::new(),
            events: vec!["next".into(), "next".into()],
        };
        let result = simulate(&machine, &config);
        let text = format_trace_text(&result);
        assert!(text.contains("TrafficLight"));
        assert!(text.contains("red"));
        assert!(text.contains("green"));
        assert!(text.contains("[next]"));
    }

    #[test]
    fn trace_format_json() {
        let machine = traffic_light();
        let config = SimConfig {
            max_steps: 10,
            initial_env: Env::new(),
            events: vec!["next".into()],
        };
        let result = simulate(&machine, &config);
        let json = format_trace_json(&result);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_object());
        assert!(parsed["trace"].is_array());
    }

    #[test]
    fn step_by_step() {
        let machine = traffic_light();
        let state = SimulationState {
            machine_name: "TrafficLight".to_string(),
            current_state: "red".to_string(),
            step: 0,
            env: Env::new(),
            trace: Vec::new(),
            status: SimStatus::Running,
        };
        let stepped = step(&machine, &state, Some("next"));
        assert_eq!(stepped.current_state, "green");
        assert_eq!(stepped.step, 1);

        let stepped2 = step(&machine, &stepped, Some("next"));
        assert_eq!(stepped2.current_state, "yellow");
    }

    #[test]
    fn parallel_regions_simulate_concurrently() {
        // Book listing-088 pattern: parallel state with two regions,
        // each with entry-then initial states and completion loops.
        let source = r#"
            state flying parallel {
                state positioning {
                    entry;
                        then stabilizing;
                    state stabilizing;
                        transition stabilizing then moving;
                    state moving;
                        transition moving then stabilizing;
                }
                state observing {
                    entry;
                        then idle;
                    state idle;
                        transition idle then watching;
                    state watching;
                        transition watching then idle;
                }
            }
        "#;
        let machines = crate::sim::state_parser::extract_state_machines("t.sysml", source);
        let flying = machines.iter().find(|m| m.name == "flying").unwrap();
        assert_eq!(flying.regions.len(), 2, "two parallel regions");

        let state = simulate(flying, &SimConfig::default());
        assert_ne!(
            state.status,
            SimStatus::Deadlocked,
            "parallel state must not deadlock: {:?}",
            state.trace
        );
        let froms: Vec<&str> = state.trace.iter().map(|s| s.from_state.as_str()).collect();
        assert!(
            froms.iter().any(|f| f.starts_with("positioning.")),
            "positioning region must step: {froms:?}"
        );
        assert!(
            froms.iter().any(|f| f.starts_with("observing.")),
            "observing region must step: {froms:?}"
        );
        assert!(
            state.current_state.contains('|'),
            "combined current state: {}",
            state.current_state
        );
    }

    #[test]
    fn transition_source_shorthand() {
        // `transition A then B;` — leading name is the source (Ch 28).
        let source = r#"
            state def M {
                entry; then a;
                state a;
                transition a then b;
                state b;
            }
        "#;
        let machines = crate::sim::state_parser::extract_state_machines("t.sysml", source);
        let m = machines.iter().find(|m| m.name == "M").unwrap();
        assert!(
            m.transitions.iter().any(|t| t.source == "a" && t.target == "b"),
            "shorthand transition extracted: {:?}",
            m.transitions
        );
    }
}
