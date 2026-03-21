/// Multi-file import resolution for SysML v2 models.
///
/// Resolves `import` statements across files in a project directory,
/// making definitions from imported packages available for type
/// checking, simulation, and linting.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::{Definition, Model};
use crate::parser;

/// A resolved project: multiple parsed files with cross-file name resolution.
#[derive(Debug)]
pub struct Project {
    pub models: Vec<Model>,
    /// Package name -> definitions available from that package.
    package_defs: HashMap<String, Vec<Definition>>,
}

impl Project {
    /// Parse all `.sysml` and `.kerml` files in the given directory (and subdirs).
    pub fn from_directory(dir: &Path) -> Self {
        let mut models = Vec::new();
        let mut files = Vec::new();
        collect_sysml_files(dir, &mut files);

        for file_path in &files {
            let path_str = file_path.to_string_lossy().to_string();
            if let Ok(source) = std::fs::read_to_string(file_path) {
                let model = parser::parse_file(&path_str, &source);
                models.push(model);
            }
        }

        let mut project = Project {
            models,
            package_defs: HashMap::new(),
        };
        project.build_package_index();
        project
    }

    /// Parse specific files and resolve imports between them.
    pub fn from_files(files: &[PathBuf]) -> Self {
        let mut models = Vec::new();

        for file_path in files {
            let path_str = file_path.to_string_lossy().to_string();
            if let Ok(source) = std::fs::read_to_string(file_path) {
                let model = parser::parse_file(&path_str, &source);
                models.push(model);
            }
        }

        let mut project = Project {
            models,
            package_defs: HashMap::new(),
        };
        project.build_package_index();
        project
    }

    /// Build an index of package -> definitions for import resolution.
    fn build_package_index(&mut self) {
        for model in &self.models {
            // Find package definitions and their contents
            let mut current_package: Option<String> = None;

            for def in &model.definitions {
                if def.kind == crate::model::DefKind::Package {
                    current_package = Some(def.name.clone());
                } else if let Some(ref pkg) = current_package {
                    self.package_defs
                        .entry(pkg.clone())
                        .or_default()
                        .push(def.clone());
                }
                // Also register under the file's implicit namespace
                let file_stem = Path::new(&model.file)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if !file_stem.is_empty() {
                    self.package_defs
                        .entry(file_stem.to_string())
                        .or_default()
                        .push(def.clone());
                }
            }
        }

        // Merge the embedded standard library packages so that
        // `import ISQ::*;` etc. resolve against real definitions.
        for (pkg, defs) in crate::stdlib::stdlib_package_defs() {
            self.package_defs
                .entry(pkg.clone())
                .or_default()
                .extend(defs.iter().cloned());
        }
    }

    /// Resolve imports for a specific model, returning all externally
    /// available definition names.
    pub fn resolve_imports(&self, model: &Model) -> Vec<String> {
        let mut resolved = Vec::new();

        for import in &model.imports {
            let path = &import.path;

            if import.is_wildcard || import.is_recursive {
                // import Vehicles::*; — find package and add all defs
                if let Some(defs) = self.package_defs.get(path) {
                    for def in defs {
                        resolved.push(def.name.clone());
                    }
                }
                // Also try matching as a prefix for nested packages
                for (pkg_name, defs) in &self.package_defs {
                    if pkg_name.starts_with(path) || path.starts_with(pkg_name) {
                        for def in defs {
                            if !resolved.contains(&def.name) {
                                resolved.push(def.name.clone());
                            }
                        }
                    }
                }
            } else {
                // import Vehicles::Car; — specific name import
                let parts: Vec<&str> = path.split("::").collect();
                if let Some(name) = parts.last() {
                    resolved.push(name.to_string());
                }
                // Also add the full qualified name
                resolved.push(path.clone());
            }
        }

        resolved
    }

    /// Get all definition names across the entire project.
    pub fn all_defined_names(&self) -> std::collections::HashSet<String> {
        let mut names = std::collections::HashSet::new();
        for model in &self.models {
            for def in &model.definitions {
                names.insert(def.name.clone());
            }
        }
        names
    }
}

fn collect_sysml_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
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
}
