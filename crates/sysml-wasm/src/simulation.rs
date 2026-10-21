//! State machine emulation for the browser: list the project's state
//! machines and step one interactively. Built on
//! `sysml_core::sim::state_chart` (structure) and
//! `sysml_core::sim::state_emulator` (stepping).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sysml_core::sim::expr::{Env, Value};
use sysml_core::sim::state_chart::{extract_state_charts, StateChart};
use sysml_core::sim::state_emulator::{self, ChartSession, EnabledTransition};
use sysml_core::sim::state_sim::{SimStatus, SimStep};
use wasm_bindgen::prelude::*;

use crate::editor::SourceFile;
use crate::to_json;

/// Every state machine in the project, in file order.
pub fn state_machines(files: &[SourceFile]) -> Vec<StateChart> {
    files
        .iter()
        .flat_map(|f| extract_state_charts(&f.path, &f.text))
        .collect()
}

/// A session as the app sends it back (the fields `stateStep` returned;
/// `enabled` and the rest are ignored).
#[derive(Debug, Deserialize)]
struct SessionIn {
    current: Vec<String>,
    #[serde(default)]
    step: usize,
    #[serde(default)]
    variables: BTreeMap<String, Json>,
    #[serde(default)]
    trace: Vec<SimStep>,
}

/// A session as `stateStep` returns it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOut {
    pub machine: String,
    /// Active leaf states (state paths, as in `stateMachines`).
    pub current: Vec<String>,
    pub step: usize,
    /// Every machine variable (`null` when unset) and any other binding.
    pub variables: BTreeMap<String, Json>,
    pub trace: Vec<SimStep>,
    /// `running`, `completed` (a top-level final state is active) or
    /// `deadlocked` (no transition leaves the active states).
    pub status: &'static str,
    /// Transitions leaving the active states, guards evaluated.
    pub enabled: Vec<EnabledTransition>,
    /// Why the step did nothing, or what it could not evaluate.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<String>,
}

fn find_machine<'a>(charts: &'a [StateChart], name: &str) -> Result<&'a StateChart, String> {
    charts
        .iter()
        .find(|c| c.qualified_name.as_deref() == Some(name))
        .or_else(|| charts.iter().find(|c| c.name == name))
        .ok_or_else(|| {
            let names: Vec<&str> = charts.iter().map(|c| c.name.as_str()).collect();
            format!(
                "no state machine named `{name}` (found: {})",
                if names.is_empty() {
                    "none".to_string()
                } else {
                    names.join(", ")
                }
            )
        })
}

fn to_value(json: &Json) -> Result<Option<Value>, String> {
    Ok(match json {
        Json::Null => None,
        Json::Bool(b) => Some(Value::Bool(*b)),
        Json::Number(n) => Some(Value::Number(
            n.as_f64().ok_or_else(|| format!("{n} is not a number"))?,
        )),
        Json::String(s) => Some(Value::String(s.clone())),
        other => return Err(format!("unsupported variable value {other}")),
    })
}

fn from_value(value: &Value) -> Json {
    match value {
        Value::Number(n) => serde_json::Number::from_f64(*n)
            .map(Json::Number)
            .unwrap_or(Json::Null),
        Value::Bool(b) => Json::Bool(*b),
        Value::String(s) => Json::String(s.clone()),
        Value::Null => Json::Null,
    }
}

/// Apply `{ name: value }` to `env`; `null` unbinds.
fn apply_variables(env: &mut Env, vars: &BTreeMap<String, Json>) -> Result<(), String> {
    for (name, json) in vars {
        match to_value(json)? {
            Some(v) => env.bind(name.clone(), v),
            None => {
                env.bindings.remove(name);
            }
        }
    }
    Ok(())
}

/// Start (`session` = `None`) or step a machine. See [`state_step`].
pub fn step(
    files: &[SourceFile],
    machine: &str,
    session: Option<&str>,
    event: Option<&str>,
    variables: Option<&str>,
) -> Result<SessionOut, String> {
    let charts = state_machines(files);
    let chart = find_machine(&charts, machine)?;
    let overrides: BTreeMap<String, Json> = match variables {
        Some(v) if !v.trim().is_empty() => {
            serde_json::from_str(v).map_err(|e| format!("variables: {e}"))?
        }
        _ => BTreeMap::new(),
    };
    let event = event.map(str::trim).filter(|e| !e.is_empty());

    let session = match session.filter(|s| !s.trim().is_empty() && s.trim() != "null") {
        None => {
            let mut env = Env::new();
            apply_variables(&mut env, &overrides)?;
            let started = state_emulator::start(chart, &env);
            match event {
                Some(_) => state_emulator::fire(chart, &started, event),
                None => started,
            }
        }
        Some(json) => {
            let input: SessionIn =
                serde_json::from_str(json).map_err(|e| format!("session: {e}"))?;
            if let Some(unknown) = input.current.iter().find(|s| chart.state(s).is_none()) {
                return Err(format!(
                    "session does not match `{}`: no state `{unknown}`",
                    chart.name
                ));
            }
            let mut env = Env::new();
            apply_variables(&mut env, &input.variables)?;
            apply_variables(&mut env, &overrides)?;
            let resumed =
                state_emulator::resume(chart, input.current, input.step, env, input.trace);
            state_emulator::fire(chart, &resumed, event)
        }
    };
    Ok(session_out(chart, session))
}

fn session_out(chart: &StateChart, session: ChartSession) -> SessionOut {
    let mut variables: BTreeMap<String, Json> = chart
        .variables
        .iter()
        .map(|v| (v.name.clone(), Json::Null))
        .collect();
    for (name, value) in session.env.iter() {
        variables.insert(name.clone(), from_value(value));
    }
    SessionOut {
        machine: chart
            .qualified_name
            .clone()
            .unwrap_or_else(|| chart.name.clone()),
        enabled: state_emulator::enabled(chart, &session),
        current: session.current,
        step: session.step,
        variables,
        trace: session.trace,
        status: match session.status {
            SimStatus::Completed => "completed",
            SimStatus::Deadlocked => "deadlocked",
            SimStatus::Running | SimStatus::MaxSteps => "running",
        },
        messages: session.messages,
    }
}

/// Every state machine in the project. `files` is JSON `[{ path, text }]`.
/// Returns `[{ name, qualifiedName?, file, line, doc?, parallel?, states:
/// [{ name, label, kind: 'initial'|'state'|'final', parent?, parallel?,
/// entry?, do?, exit?, doc?, line }], transitions: [{ name?, from, to,
/// trigger?, guard?, effect?, line }], variables: [{ name, type?, value?,
/// direction?, doc? }], warnings? }]`. State `name`s are paths within the
/// machine (`on.motors.idle`), unique per machine.
#[wasm_bindgen(js_name = stateMachines)]
pub fn state_machines_json(files: &str) -> Result<String, JsError> {
    let files: Vec<SourceFile> =
        serde_json::from_str(files).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(to_json(&state_machines(&files)))
}

/// Start or step a state machine. `files` is JSON `[{ path, text }]`;
/// `machine` its name or qualified name; `session` the JSON this function
/// last returned, or `null` to start; `event` a trigger from `enabled`
/// (`LinkLost`, `when soc < limit`, `after 5 [s]`) or `null` for
/// completion, guard-only and change transitions; `variables` JSON
/// `{ name: value }` to set before stepping (`null` unsets).
///
/// Returns `{ machine, current: string[], step, variables, trace: SimStep[],
/// status, enabled: [{ transition, index, from, to, trigger?, guard?,
/// guardResult?, conditionResult?, ready, reason? }], messages? }`.
#[wasm_bindgen(js_name = stateStep)]
pub fn state_step(
    files: &str,
    machine: &str,
    session: Option<String>,
    event: Option<String>,
    variables: Option<String>,
) -> Result<String, JsError> {
    let files: Vec<SourceFile> =
        serde_json::from_str(files).map_err(|e| JsError::new(&e.to_string()))?;
    let out = step(
        &files,
        machine,
        session.as_deref(),
        event.as_deref(),
        variables.as_deref(),
    )
    .map_err(|e| JsError::new(&e))?;
    Ok(to_json(&out))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAMP: &str = r#"package Home {
        item def Toggle;
        state def Lamp {
            attribute brightness : Real = 0.5;
            attribute switches : Integer = 0;
            entry; then off;
            state off;
            state on { entry assign switches := switches + 1; }
            transition turnOn first off accept Toggle if brightness > 0 then on;
            transition turnOff first on accept Toggle then off;
            transition burnOut first on accept when switches > 1 then done;
        }
    }"#;

    fn files() -> Vec<SourceFile> {
        vec![SourceFile {
            path: "home/Lamp.sysml".into(),
            text: LAMP.into(),
        }]
    }

    fn json(out: &SessionOut) -> String {
        serde_json::to_string(out).unwrap()
    }

    #[test]
    fn lists_machines_with_structure() {
        let charts: Json = serde_json::to_value(state_machines(&files())).unwrap();
        let lamp = &charts[0];
        assert_eq!(lamp["name"], "Lamp");
        assert_eq!(lamp["qualifiedName"], "Home::Lamp");
        assert_eq!(lamp["file"], "home/Lamp.sysml");
        assert_eq!(lamp["line"], 3);
        assert_eq!(lamp["states"][0]["kind"], "initial");
        assert_eq!(
            lamp["states"][2]["entry"],
            "assign switches := switches + 1"
        );
        assert_eq!(lamp["transitions"][1]["guard"], "brightness > 0");
        assert_eq!(lamp["transitions"][3]["trigger"], "when switches > 1");
        assert_eq!(lamp["variables"][0]["type"], "Real");
        assert_eq!(lamp["variables"][0]["value"], "0.5");
    }

    #[test]
    fn steps_through_a_session() {
        let s = step(&files(), "Lamp", None, None, None).unwrap();
        assert_eq!(s.current, vec!["off"]);
        assert_eq!(s.status, "running");
        assert_eq!(s.variables["brightness"], 0.5);
        let toggle = s.enabled.iter().find(|e| e.transition == "turnOn").unwrap();
        assert_eq!(toggle.trigger.as_deref(), Some("Toggle"));
        assert_eq!(toggle.guard_result, Some(true));

        // Dim to 0: the guard blocks the toggle.
        let s = step(
            &files(),
            "Home::Lamp",
            Some(&json(&s)),
            Some("Toggle"),
            Some(r#"{"brightness": 0}"#),
        )
        .unwrap();
        assert_eq!(s.current, vec!["off"]);
        assert!(!s.messages.is_empty());
        assert_eq!(s.enabled[0].guard_result, Some(false));

        let s = step(
            &files(),
            "Lamp",
            Some(&json(&s)),
            Some("Toggle"),
            Some(r#"{"brightness": 1}"#),
        )
        .unwrap();
        assert_eq!(s.current, vec!["on"]);
        assert_eq!(s.variables["switches"], 1.0);
        let s = step(&files(), "Lamp", Some(&json(&s)), Some("Toggle"), None).unwrap();
        let s = step(&files(), "Lamp", Some(&json(&s)), Some("Toggle"), None).unwrap();
        assert_eq!(s.variables["switches"], 2.0);
        // The change trigger now holds: a step with no event takes it.
        let s = step(&files(), "Lamp", Some(&json(&s)), None, None).unwrap();
        assert_eq!(s.current, vec!["done"]);
        assert_eq!(s.status, "completed");
        assert_eq!(s.step, 4);
        assert_eq!(s.trace.len(), 5);
    }

    #[test]
    fn rejects_unknown_machines_and_stale_sessions() {
        assert!(step(&files(), "Nope", None, None, None)
            .unwrap_err()
            .contains("Lamp"));
        assert!(step(
            &files(),
            "Lamp",
            Some(r#"{"current":["gone"]}"#),
            None,
            None
        )
        .unwrap_err()
        .contains("gone"));
        assert!(step(&files(), "Lamp", None, None, Some("[1]")).is_err());
    }
}
