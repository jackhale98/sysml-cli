//! Language features for editors: completion, hover, go to definition,
//! references, rename, outline, formatting and quick fixes.
//!
//! Transport-free and position-agnostic: everything works on byte offsets
//! into file texts. The language server (`sysml-lsp`) and the browser
//! engine (`sysml-wasm`) convert to their own positions — LSP line/UTF-16
//! columns, JavaScript string offsets.
//!
//! The project-wide features run on a [`Workspace`]: the project's files
//! indexed by [`FileIndex`] (names with exact positions, owners, types,
//! imports) plus the embedded standard library, with SysML name
//! resolution. The per-model functions in each module are the
//! language server's file-at-a-time features.

pub mod actions;
pub mod completion;
pub mod definition;
pub mod format;
pub mod hover;
pub mod index;
pub mod lex;
pub mod references;
pub mod rename;
pub mod symbols;

pub use index::{library, ElemId, ElemKind, Element, FileIndex, Src, Workspace};

use serde::Serialize;

use crate::checks;
use crate::diagnostic::Diagnostic;
use crate::model::{DefKind, Model, Span};

/// Replace bytes `start..end` of one text by `insert`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub insert: String,
}

/// Replace bytes `start..end` of file `path` by `insert`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextChange {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub insert: String,
}

/// Bytes `start..end` of file `path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Location {
    pub path: String,
    pub start: usize,
    pub end: usize,
}

/// A definition known to the language server's workspace index.
#[derive(Debug, Clone)]
pub struct WorkspaceDef {
    pub uri: String,
    pub name: String,
    pub kind: DefKind,
    pub span: Span,
    pub doc: Option<String>,
    pub super_type: Option<String>,
    pub qualified_name: Option<String>,
}

/// Check every model with project-wide name resolution, as `sysml check`
/// does: imports and root-namespace references resolve across files,
/// cross-file uses count as references, and requirements traced anywhere
/// count as traced. Returns each model (with its resolution filled in)
/// and its diagnostics, in order.
pub fn analyze_models(models: Vec<Model>) -> Vec<(Model, Vec<Diagnostic>)> {
    let project = crate::resolver::Project::from_models(models.clone());
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
            (model, diagnostics)
        })
        .collect()
}
