//! The standard `ModelViews` tables rendered against the SysML Web demo
//! model (a parcel-delivery drone), through the same JSON entry points the
//! browser calls.
//!
//! `fixtures/drone` is a verbatim copy of `src/demo/drone/**/*.sysml` from
//! the sysml-web repository (commit 6e107e3), library copies included.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "sysml") {
            out.push(path);
        }
    }
}

/// The demo project as the `[{ path, text }]` JSON the app sends.
fn drone_files() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/drone");
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    let files: Vec<Value> = paths
        .iter()
        .map(|p| {
            json!({
                "path": p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"),
                "text": std::fs::read_to_string(p).unwrap(),
            })
        })
        .collect();
    serde_json::to_string(&files).unwrap()
}

fn render(files: &str, name: &str) -> Value {
    let out = sysml_wasm::render_view(files, name)
        .unwrap_or_else(|_| panic!("view {name} should render"));
    serde_json::from_str(&out).unwrap()
}

fn column(table: &Value, name: &str) -> Vec<String> {
    let i = table["columns"]
        .as_array()
        .unwrap()
        .iter()
        .position(|c| c == name)
        .unwrap_or_else(|| panic!("no column {name}"));
    table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r[i].as_str().unwrap().to_string())
        .collect()
}

const MODEL_VIEWS: [&str; 6] = [
    "RequirementsTable",
    "RequirementGaps",
    "TraceMatrix",
    "VerificationMatrix",
    "InterfaceTable",
    "BillOfMaterials",
];

#[test]
fn every_model_view_renders_against_the_demo() {
    let files = drone_files();
    let listed: Value = serde_json::from_str(&sysml_wasm::list_views(&files).unwrap()).unwrap();
    for name in MODEL_VIEWS {
        let entry = listed
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == name)
            .unwrap_or_else(|| panic!("{name} not listed"));
        assert_eq!(entry["builtin"], true, "{entry}");
        assert_eq!(entry["hasTable"], true, "{entry}");
        assert!(entry["doc"].as_str().is_some_and(|d| !d.is_empty()));

        let t = render(&files, name);
        assert_eq!(t["view"], name);
        assert_eq!(t["warnings"], json!([]), "{name}: {}", t["warnings"]);
        assert!(
            !t["rows"].as_array().unwrap().is_empty(),
            "{name} rendered no rows"
        );
    }
}

/// The demo's own view usages with a table render too, without warnings.
#[test]
fn the_demos_table_views_render() {
    let files = drone_files();
    let listed: Value = serde_json::from_str(&sysml_wasm::list_views(&files).unwrap()).unwrap();
    let usages: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["isUsage"] == true && v["hasTable"] == true)
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert_eq!(usages, vec!["requirementCoverage", "fmeaWorksheet"]);
    for name in usages {
        let t = render(&files, name);
        assert_eq!(t["warnings"], json!([]), "{name}: {}", t["warnings"]);
        assert!(!t["rows"].as_array().unwrap().is_empty(), "{name}");
    }
    let fmea = render(&files, "DroneViews::fmeaWorksheet");
    assert_eq!(column(&fmea, "rpn")[0], "144", "highest RPN first");
}

#[test]
fn requirement_gaps_in_the_demo() {
    let t = render(&drone_files(), "RequirementGaps");
    let ids = column(&t, "id");
    // Unsatisfied: SYS-005, SYS-006 (and the stakeholder needs, which are
    // derived from, not satisfied). Unverified: SYS-011 and two risk
    // controls.
    for id in [
        "SYS-005", "SYS-006", "SYS-011", "RC-002", "RC-003", "STK-001",
    ] {
        assert!(ids.contains(&id.to_string()), "{id} missing from {ids:?}");
    }
    for id in ["SYS-001", "SYS-009", "RC-001"] {
        assert!(!ids.contains(&id.to_string()), "{id} is covered");
    }
    let row = t["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == "SYS-011")
        .unwrap();
    assert_eq!(
        row,
        &json!([
            "SYS-011",
            "Controlled parcel release",
            "1",
            "0",
            "drone.payloadRelease",
            "",
            "open",
            "K. Novak"
        ])
    );
    let i = t["rows"]
        .as_array()
        .unwrap()
        .iter()
        .position(|r| r[0] == "SYS-011")
        .unwrap();
    assert_eq!(
        t["sources"][i],
        json!({
            "element": "SystemRequirements::'Controlled parcel release'",
            "file": "requirements/SystemRequirements.sysml",
            "line": 126
        })
    );
}

#[test]
fn requirements_table_reads_the_demo_conventions() {
    let t = render(&drone_files(), "RequirementsTable");
    let i = column(&t, "id")
        .iter()
        .position(|id| id == "SYS-001")
        .unwrap();
    let get = |c: &str| column(&t, c)[i].clone();
    assert_eq!(get("title"), "Flight endurance");
    assert!(get("text").starts_with("The drone shall fly for at least 25 minutes"));
    assert!(!get("text").contains('*'));
    assert_eq!(get("kind"), "performance");
    assert_eq!(get("priority"), "high");
    assert_eq!(get("verificationMethod"), "analysis, test");
    assert_eq!(get("status"), "done");
    assert_eq!(get("owner"), "J. Hale");
    assert_eq!(get("satisfiedBy"), "drone");
    assert_eq!(get("verifiedBy"), "enduranceFlightTest");
}

#[test]
fn interface_and_bom_tables_read_the_architecture() {
    let files = drone_files();
    let ports = render(&files, "InterfaceTable");
    let rows = ports["rows"].as_array().unwrap();
    let battery = rows
        .iter()
        .find(|r| r[0] == "Battery" && r[1] == "powerOut")
        .unwrap();
    assert_eq!(
        battery,
        &json!([
            "Battery",
            "powerOut",
            "PowerPort",
            "out",
            "powerDistribution.powerIn",
            "batteryPower"
        ])
    );

    let bom = render(&files, "BillOfMaterials");
    let paths = column(&bom, "path");
    let extended = column(&bom, "extended");
    let motor = paths
        .iter()
        .position(|p| p == "drone.rotors.motor")
        .unwrap();
    assert_eq!(extended[motor], "4", "four rotors, one motor each");
}
