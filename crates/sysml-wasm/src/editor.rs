//! Editor services: highlighting, diagrams and source edits.
//!
//! Positions returned here are UTF-16 code-unit offsets — the indices of a
//! JavaScript string — so editors can apply them directly. (Model spans from
//! `analyze` stay in UTF-8 bytes, matching the engine.)

use serde::{Deserialize, Serialize};
use sysml_core::checks;
use sysml_core::codegen::edit::{self, EditPlan, TextEdit};
use sysml_core::diagnostic::Diagnostic;
use sysml_core::diagram::{self, DiagramGraph};
use sysml_core::highlight;
use sysml_core::model::Model;
use sysml_core::parser::parse_file;

/// Maps UTF-8 byte offsets of one source to UTF-16 offsets.
pub struct Utf16Index {
    /// `utf16[b]` for every char-boundary byte offset `b` (and `len`).
    utf16: Vec<u32>,
}

impl Utf16Index {
    pub fn new(source: &str) -> Self {
        let mut utf16 = vec![0u32; source.len() + 1];
        let mut units = 0u32;
        for (byte, c) in source.char_indices() {
            for slot in &mut utf16[byte..byte + c.len_utf8()] {
                *slot = units;
            }
            units += c.len_utf16() as u32;
        }
        utf16[source.len()] = units;
        Self { utf16 }
    }

    pub fn at(&self, byte: usize) -> u32 {
        self.utf16[byte.min(self.utf16.len() - 1)]
    }
}

#[derive(Serialize)]
pub struct Highlights {
    /// Capture names, e.g. `keyword`, `type.definition`.
    pub names: Vec<&'static str>,
    /// Flat `[from, to, nameIndex, from, to, nameIndex, ...]` in UTF-16 offsets.
    pub spans: Vec<u32>,
}

pub fn highlights(source: &str) -> Highlights {
    let index = Utf16Index::new(source);
    let spans = highlight::highlight(source)
        .into_iter()
        .flat_map(|s| [index.at(s.start_byte), index.at(s.end_byte), s.capture])
        .collect();
    Highlights {
        names: highlight::capture_names(),
        spans,
    }
}

#[derive(Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

/// Parse every file and combine the models, so diagrams can span files.
pub fn project_model(files: &[SourceFile]) -> Model {
    let mut merged = Model::new("project".to_owned());
    for file in files {
        let m = parse_file(&file.path, &file.text);
        merged.definitions.extend(m.definitions);
        merged.usages.extend(m.usages);
        merged.connections.extend(m.connections);
        merged.flows.extend(m.flows);
        merged.satisfactions.extend(m.satisfactions);
        merged.verifications.extend(m.verifications);
        merged.allocations.extend(m.allocations);
        merged.type_references.extend(m.type_references);
        merged.imports.extend(m.imports);
        merged.views.extend(m.views);
        merged.annotations.extend(m.annotations);
    }
    merged
}

/// One file's model and diagnostics, checked in the context of the project.
#[derive(Serialize)]
pub struct FileAnalysis {
    pub model: Model,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse and check every file with project-wide name resolution, as
/// `sysml check` does: imports and root-namespace references resolve across
/// files, cross-file uses count as references, and requirements traced
/// anywhere count as traced.
pub fn analyze_project(files: &[SourceFile]) -> Vec<FileAnalysis> {
    let models: Vec<Model> = files.iter().map(|f| parse_file(&f.path, &f.text)).collect();
    let project = sysml_core::resolver::Project::from_models(models.clone());
    let (satisfied, verified) = project.traced_requirements();
    let active = checks::all_checks();
    models
        .into_iter()
        .map(|mut model| {
            model.resolved_imports = project.resolve_imports(&model);
            model
                .resolved_imports
                .extend(project.resolve_root_refs(&model));
            model.external_references = project.external_references_for(&model);
            model.external_satisfied = satisfied.iter().cloned().collect();
            model.external_verified = verified.iter().cloned().collect();
            let diagnostics = active.iter().flat_map(|c| c.run(&model)).collect();
            FileAnalysis { model, diagnostics }
        })
        .collect()
}

/// Build a standard view of the project.
///
/// `view`: `general`, `interconnection` (needs `scope`, the owning
/// definition), `requirements`, `states`, `packages` or `trace`.
pub fn build_diagram(
    view: &str,
    model: &Model,
    scope: Option<&str>,
) -> Result<DiagramGraph, String> {
    Ok(match view {
        "general" => diagram::build_bdd(model, scope),
        "interconnection" => {
            diagram::build_ibd(model, scope.ok_or("an interconnection view needs a scope")?)
        }
        "requirements" => diagram::build_req(model),
        "states" => diagram::build_stm(model, scope),
        "packages" => diagram::build_pkg(model),
        "trace" => diagram::build_trace(model),
        other => return Err(format!("unknown view {other:?}")),
    })
}

/// An action flow view: the action named `scope` (or the first action found)
/// with its steps, successions, forks, joins, decisions and merges.
pub fn build_action_diagram(
    files: &[SourceFile],
    scope: Option<&str>,
) -> Result<DiagramGraph, String> {
    let actions: Vec<_> = files
        .iter()
        .flat_map(|f| sysml_core::sim::action_parser::extract_actions(&f.path, &f.text))
        .collect();
    let action = match scope {
        Some(name) => actions.iter().find(|a| a.name == name),
        None => actions.first(),
    }
    .ok_or_else(|| format!("no action {:?} to draw", scope.unwrap_or("")))?;
    Ok(diagram::build_act_from_action_model(action))
}

/// A source edit requested by the UI.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum EditRequest {
    /// Insert member text inside the body of the definition `parent`.
    InsertMember {
        path: String,
        source: String,
        parent: String,
        text: String,
    },
    /// Append a top-level element.
    InsertTopLevel { source: String, text: String },
    /// Remove the element named `name` (whole lines).
    Remove {
        path: String,
        source: String,
        name: String,
    },
    /// Rename `from` to `to` everywhere in the file.
    Rename {
        path: String,
        source: String,
        from: String,
        to: String,
    },
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Change {
    pub from: u32,
    pub to: u32,
    pub insert: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    /// Changes against the given source, in UTF-16 offsets, sorted.
    pub changes: Vec<Change>,
    /// Syntax errors in the edited source; the UI should not apply an edit
    /// that introduces them.
    pub syntax_errors: Vec<String>,
}

pub fn apply_request(request: EditRequest) -> Result<EditResult, String> {
    let (path, source, plan) = match request {
        EditRequest::InsertMember {
            path,
            source,
            parent,
            text,
        } => {
            let model = parse_file(&path, &source);
            let edit =
                edit::insert_member(&source, &model, &parent, &text).map_err(|e| e.message)?;
            (path, source, single(edit))
        }
        EditRequest::InsertTopLevel { source, text } => {
            let edit = edit::insert_top_level(&source, &text);
            ("edit.sysml".to_owned(), source, single(edit))
        }
        EditRequest::Remove { path, source, name } => {
            let model = parse_file(&path, &source);
            let edit = edit::remove_element(&source, &model, &name).map_err(|e| e.message)?;
            (path, source, single(edit))
        }
        EditRequest::Rename {
            path,
            source,
            from,
            to,
        } => {
            let model = parse_file(&path, &source);
            let plan = edit::rename_element(&source, &model, &from, &to).map_err(|e| e.message)?;
            (path, source, plan)
        }
    };

    let edited = edit::apply_edits(&source, &plan).map_err(|e| e.message)?;
    let syntax_errors = parse_file(&path, &edited)
        .syntax_errors
        .into_iter()
        .map(|e| e.message)
        .collect();

    let index = Utf16Index::new(&source);
    let mut changes: Vec<Change> = plan
        .edits
        .into_iter()
        .map(|e| Change {
            from: index.at(e.start_byte),
            to: index.at(e.end_byte),
            insert: e.new_text,
        })
        .collect();
    changes.sort_by_key(|c| c.from);
    Ok(EditResult {
        changes,
        syntax_errors,
    })
}

fn single(edit: TextEdit) -> EditPlan {
    let mut plan = EditPlan::new();
    plan.add(edit);
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets() {
        let index = Utf16Index::new("a°b😀c");
        assert_eq!(index.at(0), 0);
        assert_eq!(index.at(1), 1); // '°' (2 bytes, 1 unit)
        assert_eq!(index.at(3), 2); // 'b'
        assert_eq!(index.at(4), 3); // emoji (4 bytes, 2 units)
        assert_eq!(index.at(8), 5); // 'c'
        assert_eq!(index.at(9), 6);
    }

    #[test]
    fn insert_member_returns_a_valid_change() {
        let source = "part def Drone {\n    part a : A;\n}\n";
        let result = apply_request(EditRequest::InsertMember {
            path: "d.sysml".into(),
            source: source.into(),
            parent: "Drone".into(),
            text: "part b : B;".into(),
        })
        .unwrap();
        assert!(result.syntax_errors.is_empty());
        let change = &result.changes[0];
        let mut edited = source.to_owned();
        edited.replace_range(change.from as usize..change.to as usize, &change.insert);
        assert!(edited.contains("    part b : B;\n}"), "{edited}");
    }

    #[test]
    fn members_can_be_added_to_packages() {
        let source = "package P {\n    part def A;\n}\n";
        let result = apply_request(EditRequest::InsertMember {
            path: "p.sysml".into(),
            source: source.into(),
            parent: "P".into(),
            text: "part def B {\n}".into(),
        })
        .unwrap();
        assert!(result.syntax_errors.is_empty());
        let change = &result.changes[0];
        let mut edited = source.to_owned();
        edited.replace_range(change.from as usize..change.to as usize, &change.insert);
        assert_eq!(
            edited,
            "package P {\n    part def A;\n    part def B {\n    }\n}\n"
        );
    }

    #[test]
    fn edits_that_break_syntax_are_reported() {
        let result = apply_request(EditRequest::InsertTopLevel {
            source: "part def A;".into(),
            text: "part def {".into(),
        })
        .unwrap();
        assert!(!result.syntax_errors.is_empty());
    }

    #[test]
    fn rename_changes_every_whole_word_occurrence() {
        let result = apply_request(EditRequest::Rename {
            path: "d.sysml".into(),
            source: "part def Wheel; part w : Wheel; attribute wheelMass;".into(),
            from: "Wheel".into(),
            to: "Tyre".into(),
        })
        .unwrap();
        assert_eq!(result.changes.len(), 2);
    }

    #[test]
    fn project_analysis_resolves_types_across_files() {
        let files = [
            SourceFile {
                path: "a.sysml".into(),
                text:
                    "package A { private import B::*; part def Drone { part battery : Battery; } }"
                        .into(),
            },
            SourceFile {
                path: "b.sysml".into(),
                text: "package B { part def Battery; }".into(),
            },
        ];
        let single = sysml_core::checks::all_checks()
            .iter()
            .flat_map(|c| c.run(&parse_file("a.sysml", &files[0].text)))
            .filter(|d| d.code == "W004")
            .count();
        assert!(single > 0, "single-file analysis should not see Battery");
        let project = analyze_project(&files);
        assert!(
            project[0].diagnostics.iter().all(|d| d.code != "W004"),
            "{:?}",
            project[0].diagnostics
        );
    }

    #[test]
    fn diagrams_span_files() {
        let files = [
            SourceFile {
                path: "a.sysml".into(),
                text: "part def Drone { part battery : Battery; }".into(),
            },
            SourceFile {
                path: "b.sysml".into(),
                text: "part def Battery;".into(),
            },
        ];
        let graph = build_diagram("general", &project_model(&files), None).unwrap();
        let json = serde_json::to_value(&graph).unwrap();
        let labels: Vec<_> = json["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["label"].as_str().unwrap().to_owned())
            .collect();
        assert!(labels.iter().any(|l| l.contains("Drone")), "{labels:?}");
        assert!(labels.iter().any(|l| l.contains("Battery")), "{labels:?}");
        assert!(build_diagram("interconnection", &project_model(&files), None).is_err());
    }

    #[test]
    fn action_flow_diagram() {
        let files = vec![SourceFile {
            path: "a.sysml".into(),
            text:
                "action def Deliver { first start; then action fly; then action land; then done; }"
                    .into(),
        }];
        let graph = build_action_diagram(&files, Some("Deliver")).unwrap();
        assert!(graph.nodes.iter().any(|n| n.label == "fly"));
        assert!(build_action_diagram(&files, Some("Missing")).is_err());
    }
}
