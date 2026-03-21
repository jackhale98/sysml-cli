/// Embedded SysML v2 standard library.
///
/// The standard library files (`.sysml` and `.kerml`) are embedded at
/// compile time from `sysml-v2-release/sysml.library/`.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::model::{Definition, Model};
use crate::parser;

include!(concat!(env!("OUT_DIR"), "/stdlib_files.rs"));

/// Returns all embedded standard library files as (path, content) pairs.
pub fn stdlib_files() -> &'static [(&'static str, &'static str)] {
    STDLIB_FILES
}

/// Parse all embedded standard library files into Models (cached).
pub fn parse_stdlib() -> &'static [Model] {
    static MODELS: OnceLock<Vec<Model>> = OnceLock::new();
    MODELS.get_or_init(|| {
        STDLIB_FILES
            .iter()
            .map(|(path, source)| parser::parse_file(path, source))
            .collect()
    })
}

/// Collect all definition names from the standard library (cached).
pub fn stdlib_definitions() -> &'static HashSet<String> {
    static DEFS: OnceLock<HashSet<String>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let mut names = HashSet::new();
        for model in parse_stdlib() {
            for def in &model.definitions {
                names.insert(def.name.clone());
            }
            for usage in &model.usages {
                if !usage.name.is_empty() {
                    names.insert(usage.name.clone());
                }
            }
        }
        names
    })
}

/// Build a package-name -> definitions index from the standard library (cached).
pub fn stdlib_package_defs() -> &'static HashMap<String, Vec<Definition>> {
    static PKG_DEFS: OnceLock<HashMap<String, Vec<Definition>>> = OnceLock::new();
    PKG_DEFS.get_or_init(|| {
        let mut package_defs: HashMap<String, Vec<Definition>> = HashMap::new();
        for model in parse_stdlib() {
            let mut current_package: Option<String> = None;
            for def in &model.definitions {
                if def.kind == crate::model::DefKind::Package {
                    current_package = Some(def.name.clone());
                } else if let Some(ref pkg) = current_package {
                    package_defs
                        .entry(pkg.clone())
                        .or_default()
                        .push(def.clone());
                }
                // Also register under the file stem
                let file_stem = std::path::Path::new(&model.file)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if !file_stem.is_empty() {
                    package_defs
                        .entry(file_stem.to_string())
                        .or_default()
                        .push(def.clone());
                }
            }
        }
        package_defs
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdlib_files_not_empty() {
        assert!(!stdlib_files().is_empty(), "stdlib should contain files");
    }

    #[test]
    fn stdlib_contains_scalar_values() {
        let defs = stdlib_definitions();
        assert!(defs.contains("ScalarValues"), "stdlib should define ScalarValues");
    }

    #[test]
    fn stdlib_contains_isq_types() {
        let defs = stdlib_definitions();
        assert!(defs.contains("ISQ"), "stdlib should define ISQ");
    }

    #[test]
    fn stdlib_package_index_has_entries() {
        let pkg_defs = stdlib_package_defs();
        assert!(!pkg_defs.is_empty(), "stdlib package index should not be empty");
    }
}
