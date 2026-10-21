//! The SysML Web drone demo's `FlightModes` state machine
//! (`fixtures/drone/behaviour/FlightControl.sysml`), listed and emulated
//! through the JSON entry points the browser calls.

use serde_json::{json, Value};

fn files() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/drone/behaviour/FlightControl.sysml"
    );
    let text = std::fs::read_to_string(path).unwrap();
    json!([{ "path": "behaviour/FlightControl.sysml", "text": text }]).to_string()
}

fn step(files: &str, session: Option<&Value>, event: Option<&str>, vars: Option<Value>) -> Value {
    let out = sysml_wasm::simulation::state_step(
        files,
        "FlightModes",
        session.map(Value::to_string),
        event.map(str::to_string),
        vars.map(|v| v.to_string()),
    )
    .unwrap_or_else(|_| panic!("step {event:?} failed"));
    serde_json::from_str(&out).unwrap()
}

fn enabled<'a>(session: &'a Value, transition: &str) -> &'a Value {
    session["enabled"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["transition"] == transition)
        .unwrap_or_else(|| panic!("{transition} not listed: {}", session["enabled"]))
}

#[test]
fn flight_modes_structure() {
    let out = sysml_wasm::simulation::state_machines_json(&files()).unwrap();
    let machines: Value = serde_json::from_str(&out).unwrap();
    let fm = &machines[0];
    assert_eq!(fm["name"], "FlightModes");
    assert_eq!(fm["qualifiedName"], "FlightControl::FlightModes");
    assert_eq!(fm["file"], "behaviour/FlightControl.sysml");
    assert_eq!(fm["line"], 19);
    assert_eq!(fm["doc"], "Flight modes of the delivery drone.");

    let states: Vec<(&str, &str)> = fm["states"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["name"].as_str().unwrap(), s["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(
        states,
        vec![
            ("entry", "initial"),
            ("idle", "state"),
            ("armed", "state"),
            ("flying", "state"),
            ("returningHome", "state"),
            ("landing", "state"),
            ("failsafe", "state"),
        ]
    );
    assert_eq!(fm["states"][3]["do"], "followFlightPlan");
    assert_eq!(fm["states"][6]["entry"], "alertOperator");
    assert_eq!(
        fm["states"][1]["doc"],
        "On the ground with the motors off. Safe to handle."
    );

    let transitions = fm["transitions"].as_array().unwrap();
    assert_eq!(transitions.len(), 12, "initial + 11");
    assert_eq!(transitions[0]["from"], "entry");
    assert_eq!(transitions[0]["to"], "idle");
    let arm = &transitions[1];
    assert_eq!(arm["name"], "arm");
    assert_eq!(arm["trigger"], "ArmCommand");
    assert_eq!(arm["guard"], "preflightChecksPassed");
    let low = transitions
        .iter()
        .find(|t| t["name"] == "lowBattery")
        .unwrap();
    assert_eq!(low["trigger"], "when stateOfCharge < returnThreshold");
    assert_eq!(low["from"], "flying");
    assert_eq!(low["to"], "returningHome");

    assert_eq!(
        fm["variables"],
        json!([
            { "name": "preflightChecksPassed", "type": "Boolean", "direction": "in" },
            { "name": "stateOfCharge", "type": "Real", "direction": "in",
              "doc": "Battery charge left: 0 is empty, 1 is full." },
            { "name": "returnThreshold", "type": "Real", "value": "0.3",
              "doc": "Charge needed to fly home plus a 20 % reserve." },
        ])
    );
}

#[test]
fn flight_modes_emulation() {
    let files = files();

    // Start: idle, with the defaults bound.
    let s = step(&files, None, None, None);
    assert_eq!(s["machine"], "FlightControl::FlightModes");
    assert_eq!(s["current"], json!(["idle"]));
    assert_eq!(s["status"], "running");
    assert_eq!(s["variables"]["returnThreshold"], 0.3);
    assert_eq!(s["variables"]["preflightChecksPassed"], Value::Null);
    // `arm` waits for its guard, which is unset.
    let arm = enabled(&s, "arm");
    assert_eq!(arm["trigger"], "ArmCommand");
    assert_eq!(arm["ready"], false);
    assert!(arm["reason"]
        .as_str()
        .unwrap()
        .contains("preflightChecksPassed"));

    // Arming before the preflight checks pass does nothing.
    let s = step(&files, Some(&s), Some("ArmCommand"), None);
    assert_eq!(s["current"], json!(["idle"]));
    assert!(s["messages"][0].as_str().unwrap().contains("ArmCommand"));

    // Checks pass: arm, then take off.
    let s = step(
        &files,
        Some(&s),
        Some("ArmCommand"),
        Some(json!({ "preflightChecksPassed": true, "stateOfCharge": 0.9 })),
    );
    assert_eq!(s["current"], json!(["armed"]));
    let s = step(&files, Some(&s), Some("TakeOffCommand"), None);
    assert_eq!(s["current"], json!(["flying"]));

    // Plenty of charge: the low-battery change trigger is not ready, and a
    // step with no event does nothing.
    let low = enabled(&s, "lowBattery");
    assert_eq!(low["trigger"], "when stateOfCharge < returnThreshold");
    assert_eq!(low["conditionResult"], false);
    assert_eq!(low["ready"], false);
    let s = step(&files, Some(&s), None, None);
    assert_eq!(s["current"], json!(["flying"]));

    // The battery drops below the threshold: a step with no event returns.
    let s = step(
        &files,
        Some(&s),
        None,
        Some(json!({ "stateOfCharge": 0.15 })),
    );
    assert_eq!(s["current"], json!(["returningHome"]));
    let last = s["trace"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["transition_name"], "lowBattery");
    assert_eq!(last["from_state"], "flying");

    // A fault on the way home: failsafe, alerting the operator.
    let s = step(&files, Some(&s), Some("FaultDetected"), None);
    assert_eq!(s["current"], json!(["failsafe"]));
    let last = s["trace"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["entry_action"], "alertOperator");

    // Emergency landing, then a normal flight home after a lost link.
    let s = step(&files, Some(&s), Some("TouchDown"), None);
    assert_eq!(s["current"], json!(["idle"]));
    let s = step(
        &files,
        Some(&s),
        Some("ArmCommand"),
        Some(json!({ "stateOfCharge": 1.0 })),
    );
    let s = step(&files, Some(&s), Some("TakeOffCommand"), None);
    let s = step(&files, Some(&s), Some("LinkLost"), None);
    assert_eq!(s["current"], json!(["returningHome"]));
    let s = step(&files, Some(&s), Some("ArrivedHome"), None);
    assert_eq!(s["current"], json!(["landing"]));
    let s = step(&files, Some(&s), Some("TouchDown"), None);
    assert_eq!(s["current"], json!(["idle"]));

    assert_eq!(s["step"], 10, "ten transitions after the initial one");
    let path: Vec<&str> = s["trace"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["to_state"].as_str().unwrap())
        .collect();
    assert_eq!(
        path,
        vec![
            "idle",
            "armed",
            "flying",
            "returningHome",
            "failsafe",
            "idle",
            "armed",
            "flying",
            "returningHome",
            "landing",
            "idle"
        ]
    );
}
