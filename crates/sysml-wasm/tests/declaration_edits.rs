//! Editing the demo model through the inspector: `elementDetails` reads a
//! declaration, `edit` with `setDeclaration` changes it as a minimal patch,
//! and the project re-analyses cleanly.
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

fn text_of<'a>(files: &'a [Value], path: &str) -> &'a str {
    files
        .iter()
        .find(|f| f["path"] == path)
        .unwrap_or_else(|| panic!("no file {path}"))["text"]
        .as_str()
        .unwrap()
}

/// 1-based line of the first line of `path` containing `needle`.
fn line_of(files: &[Value], path: &str, needle: &str) -> usize {
    text_of(files, path)
        .lines()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("no {needle:?} in {path}"))
        + 1
}

fn details(files: &[Value], path: &str, line: usize) -> Value {
    let out =
        sysml_wasm::element_details(&Value::from(files.to_vec()).to_string(), path, line, None)
            .unwrap_or_else(|_| panic!("no element on {path}:{line}"));
    serde_json::from_str(&out).unwrap()
}

/// Run a `setDeclaration` and apply its UTF-16 changes to the file.
fn set_declaration(
    files: &mut [Value],
    path: &str,
    element: &str,
    line: usize,
    set: Value,
) -> Value {
    let request = json!({
        "op": "setDeclaration",
        "path": path,
        "source": text_of(files, path),
        "element": element,
        "line": line,
        "set": set,
    });
    let out =
        sysml_wasm::edit(&request.to_string()).unwrap_or_else(|_| panic!("edit failed: {request}"));
    let result: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(result["syntaxErrors"], json!([]), "{request}");
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

/// Every diagnostic of the project, as `code: message`, and whether any
/// file has syntax errors.
fn analyse(files: &[Value]) -> (Vec<String>, bool) {
    let out = sysml_wasm::analyze_project(&Value::from(files.to_vec()).to_string()).unwrap();
    let analyses: Vec<Value> = serde_json::from_str(&out).unwrap();
    let syntax = analyses
        .iter()
        .any(|a| !a["model"]["syntax_errors"].as_array().unwrap().is_empty());
    let mut diagnostics: Vec<String> = analyses
        .iter()
        .flat_map(|a| a["diagnostics"].as_array().unwrap().clone())
        .map(|d| format!("{} {}: {}", d["severity"], d["code"], d["message"]))
        .collect();
    diagnostics.sort();
    (diagnostics, syntax)
}

#[test]
fn the_inspector_edits_the_drone() {
    let mut files = drone();
    let (before, syntax) = analyse(&files);
    assert!(!syntax);

    // part rotors : Rotor[4];
    let arch = "system/DroneArchitecture.sysml";
    let line = line_of(&files, arch, "part rotors : Rotor[4];");
    let d = details(&files, arch, line);
    assert_eq!(
        d,
        json!({
            "name": "rotors",
            "kind": "part",
            "isDefinition": false,
            "type": "Rotor",
            "specializes": [],
            "multiplicity": "[4]",
            "qualifiedName": "DroneArchitecture::drone::rotors",
        })
    );
    let source = text_of(&files, arch).to_owned();
    let result = set_declaration(
        &mut files,
        arch,
        "rotors",
        line,
        json!({ "multiplicity": "6" }),
    );
    // One character: `4` -> `6`, at its UTF-16 offset.
    let four = source.find("Rotor[4]").unwrap() + "Rotor[".len();
    let four = source[..four].encode_utf16().count() as u64;
    assert_eq!(
        result["changes"],
        json!([{ "from": four, "to": four + 1, "insert": "6" }])
    );
    set_declaration(
        &mut files,
        arch,
        "rotors",
        line,
        json!({ "type": "DroneComponents::Rotor" }),
    );
    assert!(text_of(&files, arch).contains("        part rotors : DroneComponents::Rotor[6];\n"));
    let d = details(&files, arch, line);
    assert_eq!(
        (d["type"].as_str(), d["multiplicity"].as_str()),
        (Some("DroneComponents::Rotor"), Some("[6]"))
    );
    // The same values again change nothing.
    let again = set_declaration(
        &mut files,
        arch,
        "rotors",
        line,
        json!({ "type": "DroneComponents::Rotor", "multiplicity": "[6]" }),
    );
    assert_eq!(again["changes"], json!([]));

    // attribute returnThreshold : Real = 0.3 { doc /* ... */ }
    let fc = "behaviour/FlightControl.sysml";
    let line = line_of(&files, fc, "attribute returnThreshold");
    let d = details(&files, fc, line);
    assert_eq!(d["value"], "0.3");
    assert_eq!(d["valueKind"], "bind");
    assert_eq!(d["doc"], "Charge needed to fly home plus a 20 % reserve.");
    assert_eq!(
        d["qualifiedName"],
        "FlightControl::FlightModes::returnThreshold"
    );
    set_declaration(
        &mut files,
        fc,
        "returnThreshold",
        line,
        json!({ "value": "0.35" }),
    );
    assert!(text_of(&files, fc).contains(
        "attribute returnThreshold : Real = 0.35 { doc /* Charge needed to fly home plus a 20 % reserve. */ }"
    ));

    // port def PowerPort { out item power : ElectricPower; }
    let ifs = "interfaces/DroneInterfaces.sysml";
    let line = line_of(&files, ifs, "out item power : ElectricPower;");
    assert_eq!(details(&files, ifs, line)["direction"], "out");
    set_declaration(
        &mut files,
        ifs,
        "power",
        line,
        json!({ "direction": "inout" }),
    );
    assert!(text_of(&files, ifs).contains("        inout item power : ElectricPower;\n"));
    assert_eq!(details(&files, ifs, line)["direction"], "inout");
    // ... and back: the file is as it was.
    set_declaration(
        &mut files,
        ifs,
        "power",
        line,
        json!({ "direction": "out" }),
    );
    assert_eq!(text_of(&files, ifs), text_of(&drone(), ifs));

    // A specialization drawn in a diagram.
    let comps = "system/Components.sysml";
    let line = line_of(&files, comps, "part def Motor :> Component {");
    let d = details(&files, comps, line);
    assert_eq!(
        (d["isDefinition"].as_bool(), &d["specializes"]),
        (Some(true), &json!(["Component"]))
    );
    set_declaration(
        &mut files,
        comps,
        "Motor",
        line,
        json!({ "specializes": ["Component", "Airframe"] }),
    );
    assert!(text_of(&files, comps).contains("part def Motor :> Component, Airframe {"));
    set_declaration(
        &mut files,
        comps,
        "Motor",
        line,
        json!({ "specializes": ["Component"] }),
    );

    let (after, syntax) = analyse(&files);
    assert!(!syntax);
    assert_eq!(after, before, "the edits should not change the diagnostics");
    assert!(
        after.iter().all(|d| !d.starts_with("\"error\"")),
        "{after:#?}"
    );
}
