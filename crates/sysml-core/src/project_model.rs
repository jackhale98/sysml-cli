/// Unified multi-file project model with global definition index and type hierarchy.
///
/// `ProjectModel` merges multiple parsed `Model`s into a single queryable
/// structure. It builds a global definition index, resolves specialization
/// chains (type hierarchy), and provides project-wide queries.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::model::{simple_name, DefKind, Definition, Model, Span};
use crate::parser;

/// A definition located in a specific file.
#[derive(Debug, Clone)]
pub struct LocatedDef {
    pub file: String,
    pub name: String,
    pub kind: DefKind,
    pub span: Span,
    pub super_type: Option<String>,
    pub doc: Option<String>,
    pub parent_def: Option<String>,
}

/// Unified project model built from multiple files.
#[derive(Debug)]
pub struct ProjectModel {
    /// All individual file models.
    pub models: Vec<Model>,
    /// Global definition index: name → located definition.
    pub defs: HashMap<String, LocatedDef>,
    /// Supertypes graph: child_name → parent_name (direct specialization).
    pub supertypes: HashMap<String, String>,
    /// Subtypes graph: parent_name → Vec<child_name> (direct specializations).
    pub subtypes: HashMap<String, Vec<String>>,
}

impl ProjectModel {
    /// Build a ProjectModel from a list of already-parsed Models.
    pub fn from_models(models: Vec<Model>) -> Self {
        let mut defs = HashMap::new();
        let mut supertypes = HashMap::new();
        let mut subtypes: HashMap<String, Vec<String>> = HashMap::new();

        for model in &models {
            for def in &model.definitions {
                let located = LocatedDef {
                    file: model.file.clone(),
                    name: def.name.clone(),
                    kind: def.kind,
                    span: def.span.clone(),
                    super_type: def.super_type.clone(),
                    doc: def.doc.clone(),
                    parent_def: def.parent_def.clone(),
                };
                defs.insert(def.name.clone(), located);

                if let Some(ref st) = def.super_type {
                    let st_simple = simple_name(st).to_string();
                    supertypes.insert(def.name.clone(), st_simple.clone());
                    subtypes.entry(st_simple).or_default().push(def.name.clone());
                }
            }
        }

        ProjectModel {
            models,
            defs,
            supertypes,
            subtypes,
        }
    }

    /// Build a ProjectModel by scanning a directory for .sysml/.kerml files.
    pub fn from_directory(dir: &Path) -> Self {
        let mut files = Vec::new();
        collect_sysml_files(dir, &mut files);
        let models: Vec<Model> = files
            .iter()
            .filter_map(|path| {
                let path_str = path.to_string_lossy().to_string();
                std::fs::read_to_string(path)
                    .ok()
                    .map(|source| parser::parse_file(&path_str, &source))
            })
            .collect();
        Self::from_models(models)
    }

    /// Build from explicit file paths.
    pub fn from_files(files: &[PathBuf]) -> Self {
        let models: Vec<Model> = files
            .iter()
            .filter_map(|path| {
                let path_str = path.to_string_lossy().to_string();
                std::fs::read_to_string(path)
                    .ok()
                    .map(|source| parser::parse_file(&path_str, &source))
            })
            .collect();
        Self::from_models(models)
    }

    /// Look up a definition by name.
    pub fn find_def(&self, name: &str) -> Option<&LocatedDef> {
        self.defs.get(name)
    }

    /// Get all definition names.
    pub fn all_def_names(&self) -> Vec<&str> {
        self.defs.keys().map(|s| s.as_str()).collect()
    }

    /// Get the full supertype chain for a definition (bottom-up).
    /// Returns [immediate_super, grandparent, ...] up to the root.
    pub fn supertype_chain(&self, name: &str) -> Vec<String> {
        let mut chain = Vec::new();
        let mut current = name.to_string();
        let mut visited = HashSet::new();
        visited.insert(current.clone());

        while let Some(parent) = self.supertypes.get(&current) {
            if !visited.insert(parent.clone()) {
                break; // cycle
            }
            chain.push(parent.clone());
            current = parent.clone();
        }

        chain
    }

    /// Get all subtypes of a definition (direct children only).
    pub fn direct_subtypes(&self, name: &str) -> Vec<&str> {
        self.subtypes
            .get(name)
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get all subtypes transitively (all descendants).
    pub fn all_subtypes(&self, name: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut queue = vec![name.to_string()];
        let mut visited = HashSet::new();
        visited.insert(name.to_string());

        while let Some(current) = queue.pop() {
            if let Some(children) = self.subtypes.get(&current) {
                for child in children {
                    if visited.insert(child.clone()) {
                        result.push(child.clone());
                        queue.push(child.clone());
                    }
                }
            }
        }

        result
    }

    /// Produce a merged Model containing all definitions and usages.
    /// Useful for passing to single-model APIs (rollup, diagram, etc.).
    pub fn merged_model(&self) -> Model {
        let mut merged = Model::new("project".to_string());
        for model in &self.models {
            merged.definitions.extend(model.definitions.clone());
            merged.usages.extend(model.usages.clone());
            merged.connections.extend(model.connections.clone());
            merged.flows.extend(model.flows.clone());
            merged.satisfactions.extend(model.satisfactions.clone());
            merged.verifications.extend(model.verifications.clone());
            merged.allocations.extend(model.allocations.clone());
            merged.type_references.extend(model.type_references.clone());
            merged.imports.extend(model.imports.clone());
            merged.comments.extend(model.comments.clone());
            merged.views.extend(model.views.clone());
        }
        merged
    }

    /// Number of files in the project.
    pub fn file_count(&self) -> usize {
        self.models.len()
    }

    /// Total number of definitions across all files.
    pub fn def_count(&self) -> usize {
        self.defs.len()
    }
}

fn collect_sysml_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sysml_files(&path, files);
        } else if let Some(ext) = path.extension() {
            if ext == "sysml" || ext == "kerml" {
                files.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_project(sources: &[(&str, &str)]) -> ProjectModel {
        let models: Vec<Model> = sources
            .iter()
            .map(|(file, source)| parser::parse_file(file, source))
            .collect();
        ProjectModel::from_models(models)
    }

    #[test]
    fn index_definitions_across_files() {
        let proj = make_project(&[
            ("a.sysml", "part def Engine;"),
            ("b.sysml", "part def Vehicle { part engine : Engine; }"),
        ]);
        assert!(proj.find_def("Engine").is_some());
        assert!(proj.find_def("Vehicle").is_some());
        assert_eq!(proj.find_def("Engine").unwrap().file, "a.sysml");
        assert_eq!(proj.find_def("Vehicle").unwrap().file, "b.sysml");
    }

    #[test]
    fn supertype_chain_simple() {
        let proj = make_project(&[(
            "test.sysml",
            "part def Base;\npart def Mid :> Base;\npart def Leaf :> Mid;\n",
        )]);
        let chain = proj.supertype_chain("Leaf");
        assert_eq!(chain, vec!["Mid", "Base"]);
    }

    #[test]
    fn supertype_chain_no_parent() {
        let proj = make_project(&[("test.sysml", "part def Root;\n")]);
        let chain = proj.supertype_chain("Root");
        assert!(chain.is_empty());
    }

    #[test]
    fn supertype_chain_cycle_detection() {
        // A :> B, B :> A — should not infinite loop
        let proj = make_project(&[(
            "test.sysml",
            "part def A :> B;\npart def B :> A;\n",
        )]);
        let chain = proj.supertype_chain("A");
        // Should stop after detecting cycle
        assert!(chain.len() <= 2);
    }

    #[test]
    fn direct_subtypes() {
        let proj = make_project(&[(
            "test.sysml",
            "part def Vehicle;\npart def Car :> Vehicle;\npart def Truck :> Vehicle;\npart def Sedan :> Car;\n",
        )]);
        let subs = proj.direct_subtypes("Vehicle");
        assert!(subs.contains(&"Car"));
        assert!(subs.contains(&"Truck"));
        assert!(!subs.contains(&"Sedan")); // Sedan is a sub of Car, not Vehicle
    }

    #[test]
    fn all_subtypes_transitive() {
        let proj = make_project(&[(
            "test.sysml",
            "part def Vehicle;\npart def Car :> Vehicle;\npart def Truck :> Vehicle;\npart def Sedan :> Car;\n",
        )]);
        let all = proj.all_subtypes("Vehicle");
        assert!(all.contains(&"Car".to_string()));
        assert!(all.contains(&"Truck".to_string()));
        assert!(all.contains(&"Sedan".to_string()));
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn merged_model() {
        let proj = make_project(&[
            ("a.sysml", "part def Engine { attribute mass : Real = 100; }"),
            ("b.sysml", "part def Vehicle { part engine : Engine; }"),
        ]);
        let merged = proj.merged_model();
        assert!(merged.find_def("Engine").is_some());
        assert!(merged.find_def("Vehicle").is_some());
        assert!(!merged.usages.is_empty());
    }

    #[test]
    fn def_count_and_file_count() {
        let proj = make_project(&[
            ("a.sysml", "part def A;\npart def B;\n"),
            ("b.sysml", "part def C;\n"),
        ]);
        assert_eq!(proj.file_count(), 2);
        assert_eq!(proj.def_count(), 3);
    }

    #[test]
    fn cross_file_hierarchy() {
        let proj = make_project(&[
            ("base.sysml", "part def Base;"),
            ("derived.sysml", "part def Derived :> Base;"),
        ]);
        assert_eq!(proj.supertype_chain("Derived"), vec!["Base"]);
        assert_eq!(proj.direct_subtypes("Base"), vec!["Derived"]);
    }

    #[test]
    fn find_def_returns_none_for_missing() {
        let proj = make_project(&[("test.sysml", "part def A;\n")]);
        assert!(proj.find_def("NonExistent").is_none());
    }
}
