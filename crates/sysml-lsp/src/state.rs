use std::path::PathBuf;

use dashmap::DashMap;
use sysml_core::model::{DefKind, Model, Span};

pub struct FileState {
    pub source: String,
    pub model: Model,
    pub version: i32,
}

#[derive(Debug, Clone)]
pub struct DefLocation {
    pub uri: String,
    pub name: String,
    pub kind: DefKind,
    pub span: Span,
    pub doc: Option<String>,
    #[allow(dead_code)]
    pub super_type: Option<String>,
    #[allow(dead_code)]
    pub qualified_name: Option<String>,
}

pub struct WorldState {
    pub files: DashMap<String, FileState>,
    pub workspace_defs: DashMap<String, DefLocation>,
    #[allow(dead_code)]
    pub project_root: Option<PathBuf>,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            files: DashMap::new(),
            workspace_defs: DashMap::new(),
            project_root: None,
        }
    }

    /// Update the workspace definition index from a model's definitions.
    pub fn index_model_defs(&self, uri: &str, model: &Model) {
        // Remove old entries from this URI
        self.workspace_defs.retain(|_, v| v.uri != uri);

        for def in &model.definitions {
            let loc = DefLocation {
                uri: uri.to_string(),
                name: def.name.clone(),
                kind: def.kind,
                span: def.span.clone(),
                doc: def.doc.clone(),
                super_type: def.super_type.clone(),
                qualified_name: def.qualified_name.as_ref().map(|qn| qn.to_string()),
            };
            self.workspace_defs.insert(def.name.clone(), loc);
        }
    }
}
