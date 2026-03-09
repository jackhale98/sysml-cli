use std::path::PathBuf;
use std::process::ExitCode;

use crate::{Cli, SimulateCommand, read_source, parse_bindings, select_item, prompt_events};

pub(crate) fn run(cli: &Cli, kind: &SimulateCommand) -> ExitCode {
    match kind {
        SimulateCommand::Eval {
            file,
            bindings,
            name,
        } => run_sim_eval(cli, file, bindings, name.as_deref()),
        SimulateCommand::StateMachine {
            file,
            name,
            events,
            max_steps,
            bindings,
        } => run_sim_state_machine(cli, file, name.as_deref(), events, *max_steps, bindings),
        SimulateCommand::ActionFlow {
            file,
            name,
            max_steps,
            bindings,
        } => run_sim_action_flow(cli, file, name.as_deref(), *max_steps, bindings),
        SimulateCommand::List { file } => run_sim_list(cli, file),
    }
}

fn run_sim_eval(
    cli: &Cli,
    file: &PathBuf,
    bindings: &[String],
    name: Option<&str>,
) -> ExitCode {
    use sysml_core::sim::constraint_eval::*;
    use sysml_core::sim::eval;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let env = parse_bindings(bindings);

    let constraints = extract_constraints(&path_str, &source);
    let calcs = extract_calculations(&path_str, &source);

    let target_constraints: Vec<&ConstraintModel> = if let Some(n) = name {
        constraints.iter().filter(|c| c.name == n).collect()
    } else {
        constraints.iter().collect()
    };

    let target_calcs: Vec<&CalcModel> = if let Some(n) = name {
        calcs.iter().filter(|c| c.name == n).collect()
    } else {
        calcs.iter().collect()
    };

    if target_constraints.is_empty() && target_calcs.is_empty() {
        if let Some(n) = name {
            eprintln!("error: no constraint or calculation named `{}` found", n);
            // Suggest available items
            let available: Vec<&str> = constraints.iter().map(|c| c.name.as_str())
                .chain(calcs.iter().map(|c| c.name.as_str()))
                .collect();
            if !available.is_empty() {
                eprintln!("  available: {}", available.join(", "));
            }
        } else {
            eprintln!("error: no constraints or calculations found in `{}`", path_str);
        }
        return ExitCode::from(1);
    }

    let is_json = cli.format == "json";
    let mut results = Vec::new();

    for c in &target_constraints {
        if let Some(ref expr) = c.expression {
            let result = eval::evaluate_constraint(expr, &env);
            if is_json {
                results.push(serde_json::json!({
                    "kind": "constraint",
                    "name": c.name,
                    "result": match &result {
                        Ok(b) => serde_json::json!(b),
                        Err(e) => serde_json::json!({"error": e.message}),
                    },
                }));
            } else {
                match result {
                    Ok(b) => println!(
                        "constraint {}: {}",
                        c.name,
                        if b { "satisfied" } else { "violated" }
                    ),
                    Err(e) => println!("constraint {}: error: {}", c.name, e),
                }
            }
        }
    }

    for c in &target_calcs {
        if let Some(ref expr) = c.return_expr {
            let result = eval::evaluate(expr, &env);
            if is_json {
                results.push(serde_json::json!({
                    "kind": "calculation",
                    "name": c.name,
                    "result": match &result {
                        Ok(v) => serde_json::json!(v),
                        Err(e) => serde_json::json!({"error": e.message}),
                    },
                }));
            } else {
                match result {
                    Ok(v) => println!("calc {}: {}", c.name, v),
                    Err(e) => println!("calc {}: error: {}", c.name, e),
                }
            }
        }
    }

    if is_json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    }

    ExitCode::SUCCESS
}

fn run_sim_state_machine(
    cli: &Cli,
    file: &PathBuf,
    name: Option<&str>,
    events: &[String],
    max_steps: usize,
    bindings: &[String],
) -> ExitCode {
    use sysml_core::sim::state_machine::Trigger;
    use sysml_core::sim::state_parser::extract_state_machines;
    use sysml_core::sim::state_sim::*;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let machines = extract_state_machines(&path_str, &source);

    if machines.is_empty() {
        eprintln!("error: no state machines found in `{}`", path_str);
        return ExitCode::from(1);
    }

    let machine = if let Some(n) = name {
        match machines.iter().find(|m| m.name == n) {
            Some(m) => m,
            None => {
                eprintln!("error: no state machine named `{}` found", n);
                let available: Vec<&str> = machines.iter().map(|m| m.name.as_str()).collect();
                if !available.is_empty() {
                    eprintln!("  available: {}", available.join(", "));
                }
                return ExitCode::from(1);
            }
        }
    } else if machines.len() == 1 {
        &machines[0]
    } else {
        // Interactive selection
        match select_item("state machine", &machines.iter().map(|m| m.name.as_str()).collect::<Vec<_>>()) {
            Some(idx) => &machines[idx],
            None => return ExitCode::from(1),
        }
    };

    // Collect available signal triggers from this machine
    let available_signals: Vec<String> = machine
        .transitions
        .iter()
        .filter_map(|t| match &t.trigger {
            Some(Trigger::Signal(s)) => Some(s.clone()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // If no events provided and the machine has signal triggers, prompt interactively
    let effective_events = if events.is_empty() && !available_signals.is_empty() {
        prompt_events(&available_signals)
    } else {
        events.to_vec()
    };

    let config = SimConfig {
        max_steps,
        initial_env: parse_bindings(bindings),
        events: effective_events,
    };

    let result = simulate(machine, &config);

    let output = match cli.format.as_str() {
        "json" => format_trace_json(&result),
        _ => format_trace_text(&result),
    };
    println!("{}", output);

    match result.status {
        SimStatus::Completed | SimStatus::Running => ExitCode::SUCCESS,
        SimStatus::Deadlocked => ExitCode::from(1),
        SimStatus::MaxSteps => ExitCode::from(2),
    }
}

fn run_sim_action_flow(
    cli: &Cli,
    file: &PathBuf,
    name: Option<&str>,
    max_steps: usize,
    bindings: &[String],
) -> ExitCode {
    use sysml_core::sim::action_exec::*;
    use sysml_core::sim::action_parser::extract_actions;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let actions = extract_actions(&path_str, &source);

    if actions.is_empty() {
        eprintln!("error: no action definitions found in `{}`", path_str);
        return ExitCode::from(1);
    }

    let action = if let Some(n) = name {
        match actions.iter().find(|a| a.name == n) {
            Some(a) => a,
            None => {
                eprintln!("error: no action named `{}` found", n);
                let available: Vec<&str> = actions.iter().map(|a| a.name.as_str()).collect();
                if !available.is_empty() {
                    eprintln!("  available: {}", available.join(", "));
                }
                return ExitCode::from(1);
            }
        }
    } else if actions.len() == 1 {
        &actions[0]
    } else {
        match select_item("action", &actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>()) {
            Some(idx) => &actions[idx],
            None => return ExitCode::from(1),
        }
    };

    let config = ActionExecConfig {
        max_steps,
        initial_env: parse_bindings(bindings),
    };

    let result = execute_action(action, &config);

    let output = match cli.format.as_str() {
        "json" => format_action_trace_json(&result),
        _ => format_action_trace_text(&result),
    };
    println!("{}", output);

    match result.status {
        ActionExecStatus::Completed => ExitCode::SUCCESS,
        ActionExecStatus::Error => ExitCode::from(1),
        ActionExecStatus::MaxSteps => ExitCode::from(2),
        ActionExecStatus::Running => ExitCode::SUCCESS,
    }
}

fn run_sim_list(cli: &Cli, file: &PathBuf) -> ExitCode {
    use sysml_core::sim::action_parser::extract_actions;
    use sysml_core::sim::constraint_eval::*;
    use sysml_core::sim::state_machine::Trigger;
    use sysml_core::sim::state_parser::extract_state_machines;

    let (path_str, source) = match read_source(file) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let constraints = extract_constraints(&path_str, &source);
    let calcs = extract_calculations(&path_str, &source);
    let machines = extract_state_machines(&path_str, &source);
    let actions = extract_actions(&path_str, &source);

    if cli.format == "json" {
        // Structured JSON output for tool integration
        let json = serde_json::json!({
            "constraints": constraints.iter().map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "params": c.params.iter().map(|p| {
                        serde_json::json!({
                            "name": p.name,
                            "type": p.type_ref.as_deref().unwrap_or("?"),
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
            "calculations": calcs.iter().map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "params": c.params.iter().map(|p| {
                        serde_json::json!({
                            "name": p.name,
                            "type": p.type_ref.as_deref().unwrap_or("?"),
                        })
                    }).collect::<Vec<_>>(),
                    "return_type": c.return_type.as_deref().unwrap_or("?"),
                })
            }).collect::<Vec<_>>(),
            "state_machines": machines.iter().map(|m| {
                let triggers: Vec<&str> = m.transitions.iter()
                    .filter_map(|t| match &t.trigger {
                        Some(Trigger::Signal(s)) => Some(s.as_str()),
                        _ => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                let guards: Vec<String> = m.transitions.iter()
                    .filter(|t| t.guard.is_some())
                    .filter_map(|t| t.name.clone())
                    .collect();
                serde_json::json!({
                    "name": m.name,
                    "entry_state": m.entry_state,
                    "states": m.states.iter().map(|s| &s.name).collect::<Vec<_>>(),
                    "transitions": m.transitions.len(),
                    "triggers": triggers,
                    "guarded_transitions": guards,
                })
            }).collect::<Vec<_>>(),
            "actions": actions.iter().map(|a| {
                serde_json::json!({
                    "name": a.name,
                    "steps": a.steps.len(),
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
        return ExitCode::SUCCESS;
    }

    if constraints.is_empty() && calcs.is_empty() && machines.is_empty() && actions.is_empty() {
        println!("No simulatable constructs found in `{}`.", path_str);
        return ExitCode::SUCCESS;
    }

    if !constraints.is_empty() {
        println!("Constraints:");
        for c in &constraints {
            let params: Vec<String> = c
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.type_ref.as_deref().unwrap_or("?")))
                .collect();
            println!("  {} ({})", c.name, params.join(", "));
        }
        println!();
    }

    if !calcs.is_empty() {
        println!("Calculations:");
        for c in &calcs {
            let params: Vec<String> = c
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.type_ref.as_deref().unwrap_or("?")))
                .collect();
            let ret = c.return_type.as_deref().unwrap_or("?");
            println!("  {} ({}) -> {}", c.name, params.join(", "), ret);
        }
        println!();
    }

    if !machines.is_empty() {
        println!("State Machines:");
        for m in &machines {
            let states: Vec<&str> = m.states.iter().map(|s| s.name.as_str()).collect();
            let entry = m.entry_state.as_deref().unwrap_or("?");
            let triggers: Vec<&str> = m
                .transitions
                .iter()
                .filter_map(|t| match &t.trigger {
                    Some(Trigger::Signal(s)) => Some(s.as_str()),
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            println!(
                "  {} [entry: {}, states: {}, transitions: {}{}]",
                m.name,
                entry,
                states.join(", "),
                m.transitions.len(),
                if triggers.is_empty() {
                    String::new()
                } else {
                    format!(", triggers: {}", triggers.join(", "))
                }
            );
        }
        println!();
    }

    if !actions.is_empty() {
        println!("Actions:");
        for a in &actions {
            println!("  {} ({} steps)", a.name, a.steps.len());
        }
        println!();
    }

    ExitCode::SUCCESS
}
