//! Embedded SysML v2 standard library.
//!
//! The standard library files (`.sysml` and `.kerml`) are embedded at
//! compile time from `sysml-v2-release/sysml.library/`.

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

            // Package members are frequently *usages* (`attribute mass:
            // MassValue` in ISQ, KerML `datatype Real` in ScalarValues).
            // Index them as pseudo-definitions so `ISQ::mass` and `Real`
            // resolve, complete, and hover.
            let package_names: std::collections::HashSet<&str> = model
                .definitions
                .iter()
                .filter(|d| d.kind == crate::model::DefKind::Package)
                .map(|d| d.name.as_str())
                .collect();
            let file_stem = std::path::Path::new(&model.file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            for usage in &model.usages {
                if usage.name.is_empty() {
                    continue;
                }
                let Some(parent) = usage.parent_def.as_deref() else {
                    continue;
                };
                if !package_names.contains(parent) {
                    continue;
                }
                let pseudo = Definition {
                    kind: match usage.kind.as_str() {
                        "attribute" => crate::model::DefKind::Attribute,
                        "part" => crate::model::DefKind::Part,
                        "item" => crate::model::DefKind::Item,
                        "port" => crate::model::DefKind::Port,
                        "calc" => crate::model::DefKind::Calc,
                        "action" => crate::model::DefKind::Action,
                        _ => crate::model::DefKind::Feature,
                    },
                    name: usage.name.clone(),
                    super_type: usage.type_ref.clone(),
                    span: usage.span.clone(),
                    has_body: false,
                    param_count: 0,
                    has_constraint_expr: false,
                    has_return: false,
                    visibility: None,
                    short_name: usage.short_name.clone(),
                    doc: usage.doc.clone(),
                    is_abstract: false,
                    is_variation: false,
                    enum_members: Vec::new(),
                    parent_def: usage.parent_def.clone(),
                    body_start_byte: None,
                    body_end_byte: None,
                    qualified_name: None,
                };
                package_defs
                    .entry(parent.to_string())
                    .or_default()
                    .push(pseudo.clone());
                if !file_stem.is_empty() && file_stem != parent {
                    package_defs
                        .entry(file_stem.clone())
                        .or_default()
                        .push(pseudo);
                }
            }
        }

        // Stdlib packages re-export members via wildcard imports (`package
        // ISQ { public import ISQBase::*; }` — the book's `ISQ::mass` is
        // really ISQBase::mass). Follow wildcard imports to a fixpoint so
        // importing packages expose the members too.
        let mut pkg_imports: HashMap<String, Vec<String>> = HashMap::new();
        for model in parse_stdlib() {
            // Associate each wildcard import with its containing package by span
            for import in &model.imports {
                if !import.is_wildcard && !import.is_recursive {
                    continue;
                }
                // Only `public import` re-exports members.
                if !import.is_public {
                    continue;
                }
                let target = import
                    .path
                    .split("::")
                    .next()
                    .unwrap_or(&import.path)
                    .to_string();
                let owner = model
                    .definitions
                    .iter()
                    .filter(|d| d.kind == crate::model::DefKind::Package)
                    .filter(|d| {
                        d.span.start_byte <= import.span.start_byte
                            && import.span.end_byte <= d.span.end_byte
                    })
                    .min_by_key(|d| d.span.end_byte - d.span.start_byte);
                if let Some(owner) = owner {
                    pkg_imports
                        .entry(owner.name.clone())
                        .or_default()
                        .push(target);
                }
            }
        }
        // Iterate a few levels deep (import chains in the stdlib are short)
        for _ in 0..3 {
            let mut additions: Vec<(String, Vec<Definition>)> = Vec::new();
            for (pkg, imports) in &pkg_imports {
                for imported in imports {
                    if let Some(defs) = package_defs.get(imported) {
                        additions.push((pkg.clone(), defs.clone()));
                    }
                }
            }
            for (pkg, defs) in additions {
                let entry = package_defs.entry(pkg).or_default();
                for d in defs {
                    if !entry.iter().any(|e| e.name == d.name) {
                        entry.push(d);
                    }
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
        if stdlib_files().is_empty() {
            eprintln!("SKIP: stdlib not embedded (sysml-v2-release not available)");
            return;
        }
        assert!(!stdlib_files().is_empty());
    }

    #[test]
    fn stdlib_contains_scalar_values() {
        if stdlib_files().is_empty() {
            eprintln!("SKIP: stdlib not embedded");
            return;
        }
        let defs = stdlib_definitions();
        assert!(
            defs.contains("ScalarValues"),
            "stdlib should define ScalarValues"
        );
    }

    #[test]
    fn stdlib_contains_isq_types() {
        if stdlib_files().is_empty() {
            eprintln!("SKIP: stdlib not embedded");
            return;
        }
        let defs = stdlib_definitions();
        assert!(defs.contains("ISQ"), "stdlib should define ISQ");
    }

    #[test]
    fn stdlib_package_index_has_entries() {
        if stdlib_files().is_empty() {
            eprintln!("SKIP: stdlib not embedded");
            return;
        }
        let pkg_defs = stdlib_package_defs();
        assert!(
            !pkg_defs.is_empty(),
            "stdlib package index should not be empty"
        );
    }
}
