//! WebAssembly entry points for the SysML v2 engine.
//!
//! The browser runs this module in a Web Worker. Results cross the boundary
//! as JSON strings (parsed once on the JS side), which keeps the binding
//! surface small while the engine API is still settling. Every function is
//! also callable natively, so the protocol is tested with `cargo test`.

pub mod archive;
mod editor;
pub mod language;
pub mod library;
pub mod simulation;
pub mod views;

pub use editor::SourceFile;

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

/// Read a project archive — a `.kpar`, or any ZIP of a project such as a
/// repository download — into `{ info, files: [{path, text}], skipped,
/// warnings }`. `fallbackName` names a project without `.project.json`.
#[wasm_bindgen(js_name = readProjectArchive)]
pub fn read_project_archive(bytes: &[u8], fallback_name: &str) -> Result<String, JsError> {
    archive::read(bytes, fallback_name)
        .map(|read| to_json(&read))
        .map_err(|e| JsError::new(&e))
}

/// Write a project as a `.kpar` (KerML 10.3): `info` is the
/// `.project.json` object, `files` JSON `[{path, text}]`, `created` an ISO
/// 8601 time for `.meta.json`.
#[wasm_bindgen(js_name = writeKpar)]
pub fn write_kpar(info: &str, files: &str, created: &str) -> Result<Vec<u8>, JsError> {
    let info = serde_json::from_str(info).map_err(|e| JsError::new(&e.to_string()))?;
    let files = serde_json::from_str(files).map_err(|e| JsError::new(&e.to_string()))?;
    archive::write(info, files, created.to_owned()).map_err(|e| JsError::new(&e))
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

/// Analyse a whole project: `files` is JSON `[{ path, text }]`. Returns, in
/// the same order, `[{ model, diagnostics }]` with cross-file resolution.
#[wasm_bindgen(js_name = analyzeProject)]
pub fn analyze_project(files: &str) -> Result<String, JsError> {
    let files: Vec<editor::SourceFile> =
        serde_json::from_str(files).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(to_json(&editor::analyze_project(&files)))
}

/// Syntax highlighting for one file: `{ names, spans }` where `spans` is a
/// flat `[from, to, nameIndex, ...]` list in UTF-16 offsets.
#[wasm_bindgen]
pub fn highlight(source: &str) -> String {
    to_json(&editor::highlights(source))
}

/// A standard view of the project as a diagram graph (nodes, edges,
/// subgraphs). `files` is JSON `[{ path, text }]`; `view` is `general`,
/// `interconnection` (with `scope`), `requirements`, `states`, `packages`,
/// `trace` or `actions` (an action flow; `scope` names the action).
#[wasm_bindgen]
pub fn diagram(view: &str, files: &str, scope: Option<String>) -> Result<String, JsError> {
    let files: Vec<editor::SourceFile> =
        serde_json::from_str(files).map_err(|e| JsError::new(&e.to_string()))?;
    if view == "actions" {
        let graph =
            editor::build_action_diagram(&files, scope.as_deref()).map_err(|e| JsError::new(&e))?;
        return Ok(to_json(&graph));
    }
    let model = editor::project_model(&files);
    let graph =
        editor::build_diagram(view, &model, scope.as_deref()).map_err(|e| JsError::new(&e))?;
    Ok(to_json(&graph))
}

/// The project's views — `view def`s and `view` usages, plus the standard
/// tables of the bundled `ModelViews` library. `files` is JSON
/// `[{ path, text }]`. Returns `[{ name, qualifiedName?, file, isUsage,
/// hasTable, definition?, doc?, renderAs?, builtin }]`.
#[wasm_bindgen(js_name = listViews)]
pub fn list_views(files: &str) -> Result<String, JsError> {
    let files: Vec<editor::SourceFile> =
        serde_json::from_str(files).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(to_json(&views::list_views(&files)))
}

/// Render a view with a `@TableRendering` (own or inherited from its view
/// def) as a table. `files` is JSON `[{ path, text }]`; `name` is the
/// view's simple or qualified name. Returns `{ view, columns, rows,
/// warnings, sources, editable, availableFields }` where `rows` are string
/// cells in column order, `sources[i]` is `{ element, file, line }` for
/// `rows[i]` (empty for a pivot), `editable[j]` is `null` or `{ field,
/// kind, options? }` for `columns[j]` (see the `setField` edit) and
/// `availableFields` lists the fields the row provider offers. Throws if
/// the view is unknown or has no table.
#[wasm_bindgen(js_name = renderView)]
pub fn render_view(files: &str, name: &str) -> Result<String, JsError> {
    let files: Vec<editor::SourceFile> =
        serde_json::from_str(files).map_err(|e| JsError::new(&e.to_string()))?;
    let table = views::render_view(&files, name).map_err(|e| JsError::new(&e))?;
    Ok(to_json(&table))
}

/// The SysML libraries bundled for views (`Reporting`, `ModelViews`) as
/// JSON `[{ path, text }]`.
#[wasm_bindgen(js_name = viewLibraries)]
pub fn view_libraries() -> String {
    to_json(&views::libraries())
}

/// Compute a source edit. `request` is JSON tagged by `op`: `insertMember`
/// `{ path, source, parent, text }`, `insertTopLevel` `{ source, text }`,
/// `remove` `{ path, source, name }`, `rename` `{ path, source, from, to }`,
/// `setField` `{ path, source, element, line, field, value, files? }` (a
/// table cell: `element`/`line` from the row's `sources`, `field` from the
/// column's `editable`) or `setTableColumns` `{ path, source, view,
/// columns }`, or `setDeclaration` `{ path, source, element, line, set }`
/// where `set` is any of `type`, `conjugated`, `specializes`, `redefines`,
/// `multiplicity`, `value`, `valueKind` (`bind`, `default`, `initial`),
/// `direction`, `abstract`, `readonly`, `derived`, `end`, `shortName` and
/// `doc` (`null` removes one). Returns `{ changes: [{ from, to, insert }],
/// syntaxErrors: string[] }`.
#[wasm_bindgen]
pub fn edit(request: &str) -> Result<String, JsError> {
    let request: editor::EditRequest =
        serde_json::from_str(request).map_err(|e| JsError::new(&e.to_string()))?;
    let result = editor::apply_request(request).map_err(|e| JsError::new(&e))?;
    Ok(to_json(&result))
}

/// What an element's declaration says, for an inspector form: `files` is
/// JSON `[{ path, text }]`, `path` the file, `line` the 1-based line of the
/// declaration (the outermost one there, or the one named `element`).
/// Returns `{ name, shortName?, kind, isDefinition, type?, conjugated?,
/// specializes, redefines?, multiplicity?, value?, valueKind?, direction?,
/// abstract?, readonly?, derived?, end?, doc?, qualifiedName }`, the fields
/// `setDeclaration` takes.
#[wasm_bindgen(js_name = elementDetails)]
pub fn element_details(
    files: &str,
    path: &str,
    line: usize,
    element: Option<String>,
) -> Result<String, JsError> {
    let files: Vec<editor::SourceFile> =
        serde_json::from_str(files).map_err(|e| JsError::new(&e.to_string()))?;
    let details = editor::element_details(&files, path, line, element.as_deref())
        .map_err(|e| JsError::new(&e))?;
    Ok(to_json(&details))
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
