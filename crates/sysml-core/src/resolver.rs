//! Multi-file import resolution for SysML v2 models.
//!
//! Resolves `import` statements across files in a project directory,
//! making definitions from imported packages available for type
//! checking, simulation, and linting.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::model::{simple_name, Definition, Model};
use crate::parser;

/// Strip surrounding single quotes from an unrestricted name segment
/// (`'Library Package'` -> `Library Package`).
fn unquote(seg: &str) -> &str {
    seg.strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .unwrap_or(seg)
}

/// A resolved project: multiple parsed files with cross-file name resolution.
#[derive(Debug)]
pub struct Project {
    pub models: Vec<Model>,
    /// Package name -> definitions available from that package.
    package_defs: HashMap<String, Vec<Definition>>,
    /// Package short name (unquoted, e.g. `LIB` from `package <LIB> 'Library
    /// Package'`) -> full package name (unquoted).
    package_aliases: HashMap<String, String>,
    /// Fully-qualified names (unquoted, `::`-joined) of every definition in
    /// the project — the root namespace (SysML v2 name resolution starts at
    /// the root, so fully-qualified references resolve without any import).
    qualified_names: HashSet<String>,
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

        Self::from_models(models)
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

        Self::from_models(models)
    }

    /// Build a project from already-parsed models.
    pub fn from_models(models: Vec<Model>) -> Self {
        let mut project = Project {
            models,
            package_defs: HashMap::new(),
            package_aliases: HashMap::new(),
            qualified_names: HashSet::new(),
        };
        project.build_package_index();
        project
    }

    /// Build an index of package -> definitions for import resolution,
    /// the package short-name alias table, and the root-namespace set of
    /// fully-qualified definition names.
    fn build_package_index(&mut self) {
        let mut aliases: HashMap<String, String> = HashMap::new();
        let mut qnames: HashSet<String> = HashSet::new();

        for model in &self.models {
            // Register package short-name aliases (`package <LIB> 'Library
            // Package'` -> LIB resolves as 'Library Package').
            for def in &model.definitions {
                if def.kind == crate::model::DefKind::Package {
                    if let Some(ref sn) = def.short_name {
                        aliases.insert(
                            unquote(sn).to_string(),
                            unquote(&def.name).to_string(),
                        );
                    }
                }
            }

            // Reconstruct each definition's fully-qualified name from its
            // parent chain and add it to the root namespace.
            let mut parents: HashMap<&str, Option<&str>> = HashMap::new();
            for def in &model.definitions {
                parents.insert(def.name.as_str(), def.parent_def.as_deref());
            }
            for def in &model.definitions {
                let mut segs = vec![unquote(&def.name).to_string()];
                let mut current = def.parent_def.as_deref();
                let mut depth = 0;
                while let Some(p) = current {
                    if depth > 32 {
                        break; // guard against parent cycles
                    }
                    segs.push(unquote(p).to_string());
                    current = parents.get(p).copied().flatten();
                    depth += 1;
                }
                segs.reverse();
                qnames.insert(segs.join("::"));
            }

            // Find package definitions and their contents
            let mut current_package: Option<String> = None;

            for def in &model.definitions {
                if def.kind == crate::model::DefKind::Package {
                    current_package = Some(def.name.clone());
                } else if let Some(ref pkg) = current_package {
                    let entry = self
                        .package_defs
                        .entry(unquote(pkg).to_string())
                        .or_default();
                    entry.push(def.clone());
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

        self.package_aliases = aliases;
        self.qualified_names = qnames;
    }

    /// Normalize a `::`-qualified reference: strip quotes from each segment
    /// and expand a leading package short-name alias to the full package
    /// name (`LIB::Widget` -> `Library Package::Widget`).
    fn normalize_path(&self, path: &str) -> String {
        let mut segs: Vec<String> = path
            .split("::")
            .map(|s| unquote(s.trim()).to_string())
            .collect();
        if let Some(first) = segs.first() {
            if let Some(full) = self.package_aliases.get(first) {
                segs[0] = full.clone();
            }
        }
        segs.join("::")
    }

    /// True if a fully-qualified reference resolves in the project's root
    /// namespace (SysML v2 resolution starts at the root, so no import is
    /// required for fully-qualified names). Package short-name aliases are
    /// expanded before matching.
    pub fn resolves_from_root(&self, path: &str) -> bool {
        if !path.contains("::") {
            return false;
        }
        // Feature chains (`Pkg::Part.port`) resolve on their `::` prefix.
        let qualified_prefix = path.split('.').next().unwrap_or(path);
        self.qualified_names
            .contains(&self.normalize_path(qualified_prefix))
    }

    /// Names made visible to `model` by root-namespace resolution: for each
    /// qualified reference in the model that resolves from the root, the
    /// simple name is returned so downstream checks accept it.
    pub fn resolve_root_refs(&self, model: &Model) -> Vec<String> {
        let mut resolved = Vec::new();
        let mut push = |name: &str| {
            if self.resolves_from_root(name) {
                let simple = simple_name(name);
                if !resolved.contains(&simple.to_string()) {
                    resolved.push(simple.to_string());
                }
            }
        };
        for tr in &model.type_references {
            push(&tr.name);
        }
        for conn in &model.connections {
            push(&conn.source);
            push(&conn.target);
        }
        for alloc in &model.allocations {
            push(&alloc.source);
            push(&alloc.target);
        }
        resolved
    }

    /// Resolve imports for a specific model, returning all externally
    /// available definition names.
    pub fn resolve_imports(&self, model: &Model) -> Vec<String> {
        let mut resolved = Vec::new();

        for import in &model.imports {
            // Expand short-name aliases and strip quotes so that
            // `import LIB::*;` finds `package <LIB> 'Library Package'`.
            let path = &self.normalize_path(&import.path);

            if import.is_wildcard || import.is_recursive {
                // import Vehicles::*; — find package and add all defs
                if let Some(defs) = self.package_defs.get(path) {
                    for def in defs {
                        resolved.push(def.name.clone());
                    }
                }
                // Also try matching as a prefix for nested packages
                for (pkg_name, defs) in &self.package_defs {
                    if pkg_name.starts_with(path.as_str()) || path.starts_with(pkg_name) {
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

    /// Simple names referenced by all *other* models in the project.
    /// Used to suppress cross-file unused-definition false positives.
    pub fn external_references_for(&self, model: &Model) -> Vec<String> {
        let mut refs = HashSet::new();
        for other in &self.models {
            if other.file == model.file {
                continue;
            }
            for name in other.referenced_names() {
                refs.insert(name.to_string());
            }
        }
        refs.into_iter().collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::references::UnresolvedTypeCheck;
    use crate::checks::Check;
    use crate::parser::parse_file;

    fn project(sources: &[(&str, &str)]) -> Project {
        let models = sources
            .iter()
            .map(|(file, src)| parse_file(file, src))
            .collect();
        Project::from_models(models)
    }

    fn check_with_project(proj: &Project, file: &str) -> Vec<String> {
        let model = proj.models.iter().find(|m| m.file == file).unwrap();
        let mut model = model.clone();
        model.resolved_imports = proj.resolve_imports(&model);
        model
            .resolved_imports
            .extend(proj.resolve_root_refs(&model));
        UnresolvedTypeCheck
            .run(&model)
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn qualified_ref_resolves_without_import() {
        // SysML v2 root-namespace resolution: a fully-qualified reference
        // resolves without any import statement (book Ch 14 style).
        let proj = project(&[
            ("lib.sysml", "package LIB2 { part def Widget2; }\n"),
            ("use.sysml", "part def App { part w : LIB2::Widget2; }\n"),
        ]);
        let msgs = check_with_project(&proj, "use.sysml");
        assert!(
            msgs.iter().all(|m| !m.contains("Widget2")),
            "qualified cross-file ref must resolve without import: {msgs:?}"
        );
    }

    #[test]
    fn unresolved_qualified_ref_still_flagged() {
        let proj = project(&[
            ("lib.sysml", "package LIB2 { part def Widget2; }\n"),
            ("use.sysml", "part def App { part w : LIB2::NoSuch; }\n"),
        ]);
        let msgs = check_with_project(&proj, "use.sysml");
        assert!(
            msgs.iter().any(|m| m.contains("NoSuch")),
            "bogus qualified ref must still be flagged: {msgs:?}"
        );
    }

    #[test]
    fn short_name_wildcard_import_resolves() {
        // `package <LIB> 'Library Package'` + `import LIB::*;`
        let proj = project(&[
            (
                "lib.sysml",
                "package <LIB> 'Library Package' { part def Widget; }\n",
            ),
            (
                "use.sysml",
                "import LIB::*;\npart def App { part w : Widget; }\n",
            ),
        ]);
        let msgs = check_with_project(&proj, "use.sysml");
        assert!(
            msgs.iter().all(|m| !m.contains("Widget")),
            "short-name wildcard import must resolve: {msgs:?}"
        );
    }

    #[test]
    fn short_name_qualified_ref_resolves() {
        // `LIB::Widget` where LIB is the package's short name.
        let proj = project(&[
            (
                "lib.sysml",
                "package <LIB> 'Library Package' { part def Widget; }\n",
            ),
            ("use.sysml", "part def App { part w : LIB::Widget; }\n"),
        ]);
        let msgs = check_with_project(&proj, "use.sysml");
        assert!(
            msgs.iter().all(|m| !m.contains("Widget")),
            "short-name qualified ref must resolve: {msgs:?}"
        );
    }

    #[test]
    fn quoted_package_qualified_ref_resolves() {
        let proj = project(&[
            (
                "lib.sysml",
                "package <LIB> 'Library Package' { part def Widget; }\n",
            ),
            (
                "use.sysml",
                "part def App { part w : 'Library Package'::Widget; }\n",
            ),
        ]);
        let msgs = check_with_project(&proj, "use.sysml");
        assert!(
            msgs.iter().all(|m| !m.contains("Widget")),
            "quoted-name qualified ref must resolve: {msgs:?}"
        );
    }

    #[test]
    fn nested_package_qualified_ref_resolves() {
        let proj = project(&[
            (
                "lib.sysml",
                "package Outer { package Inner { part def Deep; } }\n",
            ),
            (
                "use.sysml",
                "part def App { part d : Outer::Inner::Deep; }\n",
            ),
        ]);
        let msgs = check_with_project(&proj, "use.sysml");
        assert!(
            msgs.iter().all(|m| !m.contains("Deep")),
            "nested qualified ref must resolve: {msgs:?}"
        );
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
