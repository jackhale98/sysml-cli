//! Model-defined views: the tables of a project, listed and rendered by the
//! engine from `view def`s / `view` usages and their `@TableRendering`.
//!
//! The standard tables (requirements, gaps, trace matrix, interfaces, bill
//! of materials) are themselves SysML, in `libraries/ModelViews.sysml`.
//! It and the `Reporting` library it builds on are bundled here and added
//! to a project that does not bring its own copy, so every project has
//! them; they are resolution context only — rows always come from the
//! project's files.

use serde::Serialize;
use sysml_core::model::{DefKind, Model};
use sysml_core::parser::parse_file;
use sysml_core::view_render::{self, RowSource, ViewInfo};

use crate::editor::SourceFile;

/// Libraries bundled with the engine: (path, package, text).
pub const BUILTIN_LIBRARIES: &[(&str, &str, &str)] = &[
    (
        "libraries/Reporting.sysml",
        "Reporting",
        include_str!("../../../libraries/Reporting.sysml"),
    ),
    (
        "libraries/ModelViews.sysml",
        "ModelViews",
        include_str!("../../../libraries/ModelViews.sysml"),
    ),
];

/// Parse the project's files and add the bundled libraries it lacks.
/// Returns the models and the paths whose content produces rows.
pub fn load(files: &[SourceFile]) -> (Vec<Model>, Vec<String>) {
    let mut models: Vec<Model> = files.iter().map(|f| parse_file(&f.path, &f.text)).collect();
    let mut targets: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    if targets.is_empty() {
        // An empty target list means "everything"; with no files that
        // would make the bundled libraries content.
        targets.push(String::new());
    }
    for (path, package, text) in BUILTIN_LIBRARIES {
        let present = models.iter().any(|m| {
            m.definitions
                .iter()
                .any(|d| d.kind == DefKind::Package && d.name == *package)
        });
        if !present {
            models.push(parse_file(path, text));
        }
    }
    (models, targets)
}

/// One entry of `listViews`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedView {
    #[serde(flatten)]
    pub info: ViewInfo,
    /// True for views from a bundled library the project does not contain.
    pub builtin: bool,
}

/// Every view in the project (and the bundled libraries).
pub fn list_views(files: &[SourceFile]) -> Vec<ListedView> {
    let (models, _) = load(files);
    view_render::list_views(&models)
        .into_iter()
        .map(|info| ListedView {
            builtin: !files.iter().any(|f| f.path == info.file),
            info,
        })
        .collect()
}

/// A rendered table with a fixed shape for the UI: every field is always
/// present.
#[derive(Serialize, Debug)]
pub struct Table {
    pub view: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub warnings: Vec<String>,
    /// Parallel to `rows` (empty for a pivot): the element behind each row.
    pub sources: Vec<RowSource>,
}

/// Render the view `name` (simple or qualified) against the project.
pub fn render_view(files: &[SourceFile], name: &str) -> Result<Table, String> {
    let (models, targets) = load(files);
    let t = view_render::render_view(&models, &targets, name)?;
    Ok(Table {
        view: t.view,
        columns: t.columns,
        rows: t.rows,
        warnings: t.warnings,
        sources: t.sources,
    })
}

/// The bundled libraries as `[{ path, text }]`, so a UI can show the
/// SysML behind a built-in table or add it to a project.
#[derive(Serialize)]
pub struct LibraryFile {
    pub path: &'static str,
    pub text: &'static str,
}

pub fn libraries() -> Vec<LibraryFile> {
    BUILTIN_LIBRARIES
        .iter()
        .map(|(path, _, text)| LibraryFile { path, text })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile {
            path: path.into(),
            text: text.into(),
        }
    }

    const PROJECT: [(&str, &str); 2] = [
        (
            "reqs.sysml",
            "package Reqs {
                requirement <'R-1'> 'Mass' { doc /* At most 4 kg. */ }
                requirement <'R-2'> 'Range' { doc /* At least 5 km. */ }
                view massOnly : ModelViews::RequirementsTable { expose Reqs::'Mass'; }
            }",
        ),
        (
            "design.sysml",
            "package Design { part drone; satisfy Reqs::'Mass' by drone; }",
        ),
    ];

    fn project() -> Vec<SourceFile> {
        PROJECT.iter().map(|(p, t)| file(p, t)).collect()
    }

    #[test]
    fn builtin_views_are_listed() {
        let views = list_views(&project());
        let gaps = views
            .iter()
            .find(|v| v.info.name == "RequirementGaps")
            .expect("bundled view");
        assert!(gaps.builtin && gaps.info.has_table && !gaps.info.is_usage);
        assert_eq!(
            gaps.info.qualified_name.as_deref(),
            Some("ModelViews::RequirementGaps")
        );
        let own = views.iter().find(|v| v.info.name == "massOnly").unwrap();
        assert!(!own.builtin && own.info.is_usage && own.info.has_table);
        assert_eq!(own.info.file, "reqs.sysml");
        let json = serde_json::to_value(own).unwrap();
        assert_eq!(json["isUsage"], true);
        assert_eq!(json["hasTable"], true);
        assert_eq!(json["definition"], "RequirementsTable");
        assert_eq!(json["qualifiedName"], "Reqs::massOnly");
    }

    #[test]
    fn gaps_render_from_the_bundled_library() {
        let t = render_view(&project(), "RequirementGaps").unwrap();
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(t.columns[0], "id");
        // Both lack verification; only R-2 lacks a satisfier.
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0][..4], ["R-1", "Mass", "1", "0"]);
        assert_eq!(t.rows[1][..4], ["R-2", "Range", "0", "0"]);
        assert_eq!(t.sources[1].element, "Reqs::Range");
        assert_eq!(t.sources[1].file, "reqs.sysml");
    }

    #[test]
    fn view_usages_render_with_their_expose() {
        let t = render_view(&project(), "massOnly").unwrap();
        assert_eq!(t.view, "massOnly");
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0][0], "R-1");
    }

    #[test]
    fn a_project_copy_of_a_library_replaces_the_bundled_one() {
        let mut files = project();
        files.push(file(
            "lib/ModelViews.sysml",
            "library package ModelViews { view def RequirementsTable; }",
        ));
        let views = list_views(&files);
        assert!(views.iter().all(|v| v.info.name != "RequirementGaps"));
        let err = render_view(&files, "RequirementsTable").unwrap_err();
        assert!(err.contains("@TableRendering"), "{err}");
    }

    #[test]
    fn unknown_views_are_errors() {
        let err = render_view(&project(), "Nope").unwrap_err();
        assert!(err.contains("RequirementGaps"), "{err}");
    }

    #[test]
    fn libraries_are_exported() {
        let libs = libraries();
        assert!(libs
            .iter()
            .any(|l| l.path.ends_with("ModelViews.sysml") && l.text.contains("RequirementGaps")));
    }
}
