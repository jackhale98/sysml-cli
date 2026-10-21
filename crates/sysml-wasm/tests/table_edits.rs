//! Editing the demo model through its tables: `renderView` says which
//! columns are editable, `edit` with `setField` / `setTableColumns` turns
//! a cell (or a column choice) into a text patch, and a re-render shows
//! the new value.
//!
//! Works on copies of `fixtures/drone` (see `drone_views.rs`).

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

/// The demo project as `[{ path, text }]`.
fn drone() -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/drone");
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    paths
        .iter()
        .map(|p| {
            json!({
                "path": p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"),
                "text": std::fs::read_to_string(p).unwrap(),
            })
        })
        .collect()
}

fn render(files: &[Value], name: &str) -> Value {
    let out = sysml_wasm::render_view(&serde_json::to_string(files).unwrap(), name)
        .unwrap_or_else(|_| panic!("view {name} should render"));
    serde_json::from_str(&out).unwrap()
}

fn col(table: &Value, name: &str) -> usize {
    table["columns"]
        .as_array()
        .unwrap()
        .iter()
        .position(|c| c == name)
        .unwrap_or_else(|| panic!("no column {name}"))
}

/// Row index whose `column` cell is `value`.
fn row(table: &Value, column: &str, value: &str) -> usize {
    let c = col(table, column);
    table["rows"]
        .as_array()
        .unwrap()
        .iter()
        .position(|r| r[c] == value)
        .unwrap_or_else(|| panic!("no row with {column} = {value}"))
}

fn cell(table: &Value, r: usize, column: &str) -> String {
    table["rows"][r][col(table, column)]
        .as_str()
        .unwrap()
        .to_string()
}

fn text_of<'a>(files: &'a [Value], path: &str) -> &'a str {
    files
        .iter()
        .find(|f| f["path"] == path)
        .unwrap_or_else(|| panic!("no file {path}"))["text"]
        .as_str()
        .unwrap()
}

/// Run an edit request and apply its UTF-16 changes to the file it names.
fn apply(files: &mut [Value], request: Value) -> Value {
    let out =
        sysml_wasm::edit(&request.to_string()).unwrap_or_else(|_| panic!("edit failed: {request}"));
    let result: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(result["syntaxErrors"], json!([]), "{request}");
    let path = request["path"].as_str().unwrap();
    let file = files.iter_mut().find(|f| f["path"] == path).unwrap();
    let mut units: Vec<u16> = file["text"].as_str().unwrap().encode_utf16().collect();
    let mut changes = result["changes"].as_array().unwrap().clone();
    changes.reverse();
    for c in changes {
        let (from, to) = (
            c["from"].as_u64().unwrap() as usize,
            c["to"].as_u64().unwrap() as usize,
        );
        let insert: Vec<u16> = c["insert"].as_str().unwrap().encode_utf16().collect();
        units.splice(from..to, insert);
    }
    file["text"] = json!(String::from_utf16(&units).unwrap());
    result
}

/// A `setField` request for row `r` of `table`, column `column`.
fn set_field(files: &[Value], table: &Value, r: usize, column: &str, value: &str) -> Value {
    let source = &table["sources"][r];
    let path = source["file"].as_str().unwrap();
    json!({
        "op": "setField",
        "path": path,
        "source": text_of(files, path),
        "element": source["element"],
        "line": source["line"],
        "field": table["editable"][col(table, column)]["field"],
        "value": value,
        "files": files,
    })
}

/// Edit row `r`'s `column` cell to `value` and apply the change.
fn edit_cell(files: &mut [Value], table: &Value, r: usize, column: &str, value: &str) -> Value {
    let request = set_field(files, table, r, column, value);
    apply(files, request)
}

#[test]
fn the_requirements_table_says_what_is_editable() {
    let t = render(&drone(), "RequirementsTable");
    let editable = |c: &str| t["editable"][col(&t, c)].clone();
    assert_eq!(
        editable("id"),
        json!({ "field": "shortName", "kind": "text" })
    );
    assert_eq!(
        editable("title"),
        json!({ "field": "name", "kind": "text" })
    );
    assert_eq!(
        editable("text"),
        json!({ "field": "doc", "kind": "longText" })
    );
    assert_eq!(
        editable("priority"),
        json!({ "field": "feature:priority", "kind": "enum", "options": ["high", "medium", "low"] })
    );
    assert_eq!(
        editable("kind")["options"],
        json!([
            "functional",
            "performance",
            "interface",
            "safety",
            "regulatory",
            "environmental"
        ])
    );
    assert_eq!(
        editable("verificationMethod"),
        json!({
            "field": "feature:verificationMethod",
            "kind": "enumList",
            "options": ["test", "analysis", "inspection", "demonstration"]
        })
    );
    assert_eq!(
        editable("status"),
        json!({
            "field": "metadata:StatusInfo.status",
            "kind": "enum",
            "options": ["open", "tbd", "tbr", "tbc", "done", "closed"]
        })
    );
    assert_eq!(
        editable("owner"),
        json!({ "field": "metadata:StatusInfo.owner", "kind": "text" })
    );
    assert_eq!(editable("satisfiedBy"), Value::Null);
    assert_eq!(editable("verifiedBy"), Value::Null);
    let fields = t["availableFields"].as_array().unwrap();
    for f in ["id", "rationale", "satisfied", "issues", "kind"] {
        assert!(fields.contains(&json!(f)), "{f} missing from {fields:?}");
    }

    let gaps = render(&drone(), "RequirementGaps");
    assert_eq!(gaps["editable"][col(&gaps, "satisfied")], Value::Null);
}

#[test]
fn editing_sys_001_priority_and_status() {
    let mut files = drone();
    let t = render(&files, "RequirementsTable");
    let r = row(&t, "id", "SYS-001");
    assert_eq!(cell(&t, r, "priority"), "high");
    assert_eq!(cell(&t, r, "status"), "done");

    let result = edit_cell(&mut files, &t, r, "priority", "low");
    // One minimal change: the literal, qualified as before.
    assert_eq!(result["changes"].as_array().unwrap().len(), 1);
    assert_eq!(result["changes"][0]["insert"], "Priority::low");
    edit_cell(&mut files, &t, r, "status", "open");

    let text = text_of(&files, "requirements/SystemRequirements.sysml");
    assert!(text.contains("        :>> priority = Priority::low;\n"));
    assert!(text.contains("@StatusInfo { status = StatusKind::open; owner = \"J. Hale\"; }"));

    let after = render(&files, "RequirementsTable");
    let r = row(&after, "id", "SYS-001");
    assert_eq!(cell(&after, r, "priority"), "low");
    assert_eq!(cell(&after, r, "status"), "open");
    assert_eq!(cell(&after, r, "owner"), "J. Hale");

    // The other cells of the row: verification methods, owner, text, ID.
    edit_cell(&mut files, &after, r, "verificationMethod", "test");
    edit_cell(&mut files, &after, r, "owner", "R. Tanaka");
    edit_cell(
        &mut files,
        &after,
        r,
        "text",
        "The drone shall fly for 30 minutes.",
    );
    edit_cell(&mut files, &after, r, "id", "SYS-101");
    let again = render(&files, "RequirementsTable");
    let r = row(&again, "id", "SYS-101");
    assert_eq!(cell(&again, r, "verificationMethod"), "test");
    assert_eq!(cell(&again, r, "owner"), "R. Tanaka");
    assert_eq!(
        cell(&again, r, "text"),
        "The drone shall fly for 30 minutes."
    );
    assert_eq!(cell(&again, r, "title"), "Flight endurance");
}

#[test]
fn a_new_value_takes_its_type_from_the_project() {
    let mut files = drone();
    files.push(json!({
        "path": "requirements/New.sysml",
        "text": "package NewRequirements {\n    private import EngineeringBasics::*;\n\n    requirement <'NEW-1'> 'New need' : EngineeringRequirement;\n}\n",
    }));
    let t = render(&files, "RequirementsTable");
    let r = row(&t, "id", "NEW-1");
    assert_eq!(cell(&t, r, "priority"), "");

    // Without the project the file alone cannot tell `low` is a Priority.
    let mut alone = set_field(&files, &t, r, "priority", "low");
    alone.as_object_mut().unwrap().remove("files");
    let out: Value = serde_json::from_str(&sysml_wasm::edit(&alone.to_string()).unwrap()).unwrap();
    assert!(out["changes"][0]["insert"]
        .as_str()
        .unwrap()
        .contains(":>> priority = \"low\";"));

    edit_cell(&mut files, &t, r, "priority", "low");
    assert_eq!(
        text_of(&files, "requirements/New.sysml"),
        "package NewRequirements {\n    private import EngineeringBasics::*;\n\n    requirement <'NEW-1'> 'New need' : EngineeringRequirement {\n        :>> priority = Priority::low;\n    }\n}\n"
    );
    let after = render(&files, "RequirementsTable");
    assert_eq!(cell(&after, row(&after, "id", "NEW-1"), "priority"), "low");
}

#[test]
fn choosing_the_columns_of_a_view() {
    let mut files = drone();
    let before = render(&files, "requirementCoverage");
    assert_eq!(before["columns"][0], "requirement");
    let fields = before["availableFields"].as_array().unwrap();
    assert!(fields.contains(&json!("owner")));
    let request = json!({
        "op": "setTableColumns",
        "path": "views/Views.sysml",
        "source": text_of(&files, "views/Views.sysml"),
        "view": "DroneViews::requirementCoverage",
        "columns": ["id", "requirement", "owner"],
    });
    apply(&mut files, request);
    assert!(text_of(&files, "views/Views.sysml")
        .contains("@TableRendering { columns = \"id; requirement; owner\"; }"));
    let after = render(&files, "requirementCoverage");
    assert_eq!(after["columns"], json!(["id", "requirement", "owner"]));
    assert_eq!(
        after["rows"].as_array().unwrap().len(),
        before["rows"].as_array().unwrap().len()
    );
    assert_eq!(after["editable"][2]["field"], "metadata:StatusInfo.owner");
}

#[test]
fn fmea_annotation_rows_edit_their_fields() {
    let mut files = drone();
    let t = render(&files, "fmeaWorksheet");
    let editable = |c: &str| t["editable"][col(&t, c)].clone();
    assert_eq!(editable("element"), Value::Null);
    assert_eq!(editable("rpn"), Value::Null);
    assert_eq!(
        editable("failureMode"),
        json!({ "field": "metadata:Fmea.failureMode", "kind": "text" })
    );
    assert_eq!(
        editable("severity"),
        json!({ "field": "metadata:Fmea.severity", "kind": "number" })
    );
    assert_eq!(
        editable("category"),
        json!({
            "field": "metadata:Fmea.category",
            "kind": "enum",
            "options": ["design", "process", "use", "software"]
        })
    );

    let r = row(&t, "failureMode", "Thermal runaway");
    let rpn = cell(&t, r, "rpn");
    edit_cell(&mut files, &t, r, "likelihood", "1");
    edit_cell(&mut files, &t, r, "category", "use");
    assert!(text_of(&files, "risk/DesignFmea.sysml").contains("category = RiskCategory::'use';"));
    let after = render(&files, "fmeaWorksheet");
    let r = row(&after, "failureMode", "Thermal runaway");
    assert_eq!(cell(&after, r, "likelihood"), "1");
    // Annotation rows show enum values qualified, as written.
    assert_eq!(cell(&after, r, "category"), "RiskCategory::use");
    assert_eq!(
        cell(&after, r, "rpn").parse::<u32>().unwrap() * 3,
        rpn.parse::<u32>().unwrap()
    );
}
