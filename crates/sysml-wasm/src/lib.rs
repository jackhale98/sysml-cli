//! WebAssembly entry points for the SysML v2 engine.
//!
//! The browser runs this module in a Web Worker. Results cross the boundary
//! as JSON strings (parsed once on the JS side), which keeps the binding
//! surface small while the engine API is still settling. Every function is
//! also callable natively, so the protocol is tested with `cargo test`.

mod editor;

use serde::Serialize;
use sysml_core::checks;
use sysml_core::diagnostic::Diagnostic;
use sysml_core::model::Model;
use sysml_interchange::{ChecksumProblem, Kpar, ProjectInfo, ProjectMeta};
use wasm_bindgen::prelude::*;

/// Engine version (the workspace version).
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[derive(Serialize)]
struct Analysis<'a> {
    model: &'a Model,
    diagnostics: Vec<Diagnostic>,
}

/// Parse one file and run the single-file checks.
///
/// Returns `{ "model": Model, "diagnostics": Diagnostic[] }` as JSON.
#[wasm_bindgen]
pub fn analyze(path: &str, source: &str) -> String {
    let model = sysml_core::parser::parse_file(path, source);
    let diagnostics = checks::all_checks()
        .iter()
        .flat_map(|check| check.run(&model))
        .collect();
    to_json(&Analysis {
        model: &model,
        diagnostics,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KparSummary<'a> {
    info: &'a ProjectInfo,
    meta: &'a ProjectMeta,
    files: Vec<&'a str>,
    checksum_problems: Vec<String>,
}

/// Read a project interchange file (`.kpar`).
///
/// Returns `{ "info", "meta", "files": string[], "checksumProblems": string[] }`
/// as JSON, or throws with a message if the archive is invalid.
#[wasm_bindgen(js_name = readKpar)]
pub fn read_kpar(bytes: &[u8]) -> Result<String, JsError> {
    let kpar = Kpar::read(bytes).map_err(|e| JsError::new(&e.to_string()))?;
    let checksum_problems = kpar
        .verify_checksums()
        .into_iter()
        .map(|problem| match problem {
            ChecksumProblem::Mismatch { path } => format!("{path}: checksum mismatch"),
            ChecksumProblem::MissingFile { path } => format!("{path}: listed but missing"),
            ChecksumProblem::Unsupported { path, algorithm } => {
                format!("{path}: {algorithm} not verified")
            }
        })
        .collect();
    Ok(to_json(&KparSummary {
        info: &kpar.info,
        meta: &kpar.meta,
        files: kpar.files.keys().map(String::as_str).collect(),
        checksum_problems,
    }))
}

/// `elementId` of an element in a standard library package, from its
/// `path()` (KerML 9.1). `spec` is `"KerML"` or `"SysML"`.
#[wasm_bindgen(js_name = libraryElementId)]
pub fn library_element_id(spec: &str, package: &str, path: &str) -> Result<String, JsError> {
    use sysml_semantic::{ElementId, Spec};
    let spec = match spec {
        "KerML" => Spec::KerML,
        "SysML" => Spec::SysML,
        other => return Err(JsError::new(&format!("unknown spec {other:?}"))),
    };
    let package_id = ElementId::library_package(spec, package);
    Ok(if path.is_empty() {
        package_id
    } else {
        ElementId::contained(package_id, path)
    }
    .to_string())
}

/// Syntax highlighting for one file: `{ names, spans }` where `spans` is a
/// flat `[from, to, nameIndex, ...]` list in UTF-16 offsets.
#[wasm_bindgen]
pub fn highlight(source: &str) -> String {
    to_json(&editor::highlights(source))
}

/// A standard view of the project as a diagram graph (nodes, edges,
/// subgraphs). `files` is JSON `[{ path, text }]`; `view` is `general`,
/// `interconnection` (with `scope`), `requirements`, `states`, `packages` or
/// `trace`.
#[wasm_bindgen]
pub fn diagram(view: &str, files: &str, scope: Option<String>) -> Result<String, JsError> {
    let files: Vec<editor::SourceFile> =
        serde_json::from_str(files).map_err(|e| JsError::new(&e.to_string()))?;
    let model = editor::project_model(&files);
    let graph =
        editor::build_diagram(view, &model, scope.as_deref()).map_err(|e| JsError::new(&e))?;
    Ok(to_json(&graph))
}

/// Compute a source edit. `request` is JSON tagged by `op`: `insertMember`
/// `{ path, source, parent, text }`, `insertTopLevel` `{ source, text }`,
/// `remove` `{ path, source, name }` or `rename` `{ path, source, from, to }`.
/// Returns `{ changes: [{ from, to, insert }], syntaxErrors: string[] }`.
#[wasm_bindgen]
pub fn edit(request: &str) -> Result<String, JsError> {
    let request: editor::EditRequest =
        serde_json::from_str(request).map_err(|e| JsError::new(&e.to_string()))?;
    let result = editor::apply_request(request).map_err(|e| JsError::new(&e))?;
    Ok(to_json(&result))
}

fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("engine results serialize to JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_reports_model_and_diagnostics() {
        let out: serde_json::Value = serde_json::from_str(&analyze(
            "drone.sysml",
            "package Drone { part def Battery; part def Drone { part battery : Battery; } }",
        ))
        .unwrap();
        let names: Vec<&str> = out["model"]["definitions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|d| d["name"].as_str())
            .collect();
        assert!(names.contains(&"Battery"), "{names:?}");
        assert!(out["diagnostics"].is_array());
    }

    #[test]
    fn analyze_reports_syntax_errors() {
        let out: serde_json::Value =
            serde_json::from_str(&analyze("bad.sysml", "part def {")).unwrap();
        assert!(!out["model"]["syntax_errors"].as_array().unwrap().is_empty());
    }

    #[test]
    fn library_ids() {
        assert_eq!(
            library_element_id("SysML", "Parts", "Parts/1/1").unwrap(),
            "2b574b4a-b51c-560b-bf14-33cf613dc44c"
        );
    }
}
