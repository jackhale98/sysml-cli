//! Browse and search the OMG SysML v2 standard library embedded in the
//! engine (`sysml_core::stdlib::STDLIB_FILES`, from the `sysml-v2-release`
//! submodule).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Serialize;
use sysml_core::model::Model;
use sysml_core::stdlib;
use wasm_bindgen::prelude::*;

use crate::to_json;

/// The embedded OMG standard library files: `[{ path, package, kind:
/// 'kerml'|'sysml', size }]`, paths like `Systems Library/Ports.sysml`.
#[wasm_bindgen(js_name = libraryIndex)]
pub fn library_index() -> String {
    to_json(&index())
}

/// One standard library file: `{ path, text }`. Throws for an unknown path.
#[wasm_bindgen(js_name = libraryFile)]
pub fn library_file(path: &str) -> Result<String, JsError> {
    file(path)
        .map(|f| to_json(&f))
        .ok_or_else(|| JsError::new(&format!("no library file {path:?}")))
}

/// Search the standard library's definitions and usages by name: `[{ name,
/// qualifiedName, kind, isDefinition, path, line, doc? }]`, exact matches
/// first, then prefix matches, then the rest; at most `limit`.
#[wasm_bindgen(js_name = librarySearch)]
pub fn library_search(query: &str, limit: usize) -> String {
    to_json(&search(query, limit))
}

/// Licence of the embedded library: `{ spdx, notice, source, licenseUrl }`.
#[wasm_bindgen(js_name = libraryLicense)]
pub fn library_license() -> String {
    to_json(&license())
}

/// One library file, without its text.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryEntry {
    /// Path inside the library, e.g. `Systems Library/Ports.sysml`.
    pub path: &'static str,
    /// The package the file declares.
    pub package: String,
    /// `kerml` or `sysml`, from the extension.
    pub kind: &'static str,
    /// Size of the text in bytes.
    pub size: usize,
}

/// One library file with its text.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryFile {
    pub path: &'static str,
    pub text: &'static str,
}

/// A definition or usage found by [`search`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryHit {
    pub name: String,
    pub qualified_name: String,
    /// `part def`, `attribute`, `package`, ...
    pub kind: String,
    pub is_definition: bool,
    pub path: String,
    /// 1-based line of the declaration.
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Licence of the embedded library.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryLicense {
    pub spdx: &'static str,
    pub notice: &'static str,
    /// Where the library comes from.
    pub source: &'static str,
    pub license_url: &'static str,
}

/// The licence notice of the SysML v2 Release (its README, "Licensing"):
/// the KerML and SysML models, the standard library included, are LGPL v3.0.
const NOTICE: &str = "\
The OMG SysML v2 standard library (KerML and SysML model libraries) is \
embedded from the SysML v2 Release, https://github.com/Systems-Modeling/SysML-v2-Release. \
It is licensed by the respective copyright holders listed below under the GNU \
Lesser General Public License (LGPL) v3.0 (see the LICENSE and LICENSE-GPL \
files of that repository).

Copyright © 2019-2021 California Institute of Technology (Jet Propulsion Laboratory)
Copyright © 2019-2025 DEKonsult
Copyright © 2019-2021 IncQuery Labs Ltd.
Copyright © 2019-2025 Intercax LLC
Copyright © 2019-2021 Itemis
Copyright © 2019-2021 Maplesoft (Waterloo Maple, Inc.)
Copyright © 2019-2025 Mgnite Inc.
Copyright © 2019-2025 Model Driven Solutions, Inc.
Copyright © 2019-2025 SAF Consulting
Copyright © 2022 Siemens AG
Copyright © 2021-2025 Twingineer LLC";

pub fn license() -> LibraryLicense {
    LibraryLicense {
        spdx: "LGPL-3.0-only",
        notice: NOTICE,
        source: "https://github.com/Systems-Modeling/SysML-v2-Release",
        license_url: "https://www.gnu.org/licenses/lgpl-3.0.html",
    }
}

/// Every embedded file, in path order.
pub fn index() -> Vec<LibraryEntry> {
    stdlib::stdlib_files()
        .iter()
        .map(|(path, text)| LibraryEntry {
            path,
            package: declared_package(text).unwrap_or_default(),
            kind: if path.ends_with(".kerml") {
                "kerml"
            } else {
                "sysml"
            },
            size: text.len(),
        })
        .collect()
}

/// One file by its library path.
pub fn file(path: &str) -> Option<LibraryFile> {
    stdlib::stdlib_files()
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(path, text)| LibraryFile { path, text })
}

/// The name in the first `package` declaration of a file
/// (`standard library package ISQ {`, `package 'Quantities and Units'`).
fn declared_package(text: &str) -> Option<String> {
    let mut in_comment = false;
    for line in text.lines() {
        let mut line = line.trim();
        if in_comment {
            match line.find("*/") {
                Some(end) => {
                    in_comment = false;
                    line = line[end + 2..].trim();
                }
                None => continue,
            }
        }
        if line.starts_with("//") {
            continue;
        }
        if line.starts_with("/*") && !line.contains("*/") {
            in_comment = true;
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some(i) = words.iter().position(|w| *w == "package") else {
            continue;
        };
        if !words[..i]
            .iter()
            .all(|w| matches!(*w, "standard" | "library" | "public" | "private"))
        {
            continue;
        }
        let rest = line.split_once("package")?.1.trim();
        let name = if let Some(quoted) = rest.strip_prefix('\'') {
            quoted.split('\'').next()?.to_string()
        } else {
            rest.split(|c: char| c.is_whitespace() || c == '{' || c == ';')
                .next()?
                .to_string()
        };
        return (!name.is_empty()).then_some(name);
    }
    None
}

/// Search the library's definitions and usages by name (case-insensitive).
/// Exact matches come first, then prefix matches, then names containing the
/// query; definitions before usages, then shorter names. A query with `::`
/// matches qualified names instead. At most `limit` hits.
pub fn search(query: &str, limit: usize) -> Vec<LibraryHit> {
    let query = query.trim().to_lowercase();
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }
    let qualified = query.contains("::");
    let mut ranked: Vec<(u8, &LibraryHit)> = items()
        .iter()
        .filter_map(|item| {
            let hay = if qualified {
                item.qualified_name.to_lowercase()
            } else {
                item.name.to_lowercase()
            };
            let rank = if hay == query {
                0
            } else if hay.starts_with(&query) || (qualified && hay.ends_with(&query)) {
                1
            } else if hay.contains(&query) {
                2
            } else {
                return None;
            };
            Some((rank, item))
        })
        .collect();
    ranked.sort_by(|(ra, a), (rb, b)| {
        ra.cmp(rb)
            .then(b.is_definition.cmp(&a.is_definition))
            .then(a.name.len().cmp(&b.name.len()))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.qualified_name.cmp(&b.qualified_name))
    });
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, item)| item.clone())
        .collect()
}

/// Every named definition and usage of the library (built once, from the
/// cached `parse_stdlib()`).
fn items() -> &'static [LibraryHit] {
    static ITEMS: OnceLock<Vec<LibraryHit>> = OnceLock::new();
    ITEMS.get_or_init(|| {
        stdlib::parse_stdlib()
            .iter()
            .flat_map(model_items)
            .collect()
    })
}

fn model_items(model: &Model) -> Vec<LibraryHit> {
    // Parent chains run through named definitions (and packages).
    let parents: HashMap<&str, Option<&str>> = model
        .definitions
        .iter()
        .map(|d| (d.name.as_str(), d.parent_def.as_deref()))
        .collect();
    let qualify = |name: &str, parent: Option<&str>| {
        let mut segments = vec![name.to_string()];
        let mut current = parent;
        while let Some(p) = current {
            if segments.len() > 32 || segments.iter().any(|s| s == p) {
                break;
            }
            segments.push(p.to_string());
            current = parents.get(p).copied().flatten();
        }
        segments.reverse();
        segments.join("::")
    };
    let mut out = Vec::new();
    for d in &model.definitions {
        if d.name.is_empty() {
            continue;
        }
        out.push(LibraryHit {
            name: d.name.clone(),
            qualified_name: qualify(&d.name, d.parent_def.as_deref()),
            kind: d.kind.label().to_string(),
            is_definition: true,
            path: model.file.clone(),
            line: d.span.start_row,
            doc: d.doc.clone(),
        });
    }
    for u in &model.usages {
        if u.name.is_empty() {
            continue;
        }
        out.push(LibraryHit {
            name: u.name.clone(),
            qualified_name: qualify(&u.name, u.parent_def.as_deref()),
            kind: u.kind.clone(),
            is_definition: false,
            path: model.file.clone(),
            line: u.span.start_row,
            doc: u.doc.clone(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embedded() -> bool {
        !stdlib::stdlib_files().is_empty()
    }

    #[test]
    fn index_lists_files_with_packages() {
        if !embedded() {
            return;
        }
        let index = index();
        let ports = index
            .iter()
            .find(|e| e.path == "Systems Library/Ports.sysml")
            .expect("Ports.sysml is in the index");
        assert_eq!(ports.package, "Ports");
        assert_eq!(ports.kind, "sysml");
        assert!(ports.size > 0);
        assert!(index.iter().any(|e| e.kind == "kerml"));
        assert!(
            index.iter().all(|e| !e.package.is_empty()),
            "every file declares a package: {:?}",
            index.iter().find(|e| e.package.is_empty())
        );
    }

    #[test]
    fn file_returns_text_by_path() {
        if !embedded() {
            return;
        }
        let f = file("Systems Library/Ports.sysml").unwrap();
        assert!(f.text.contains("package Ports"));
        assert!(file("nope.sysml").is_none());
    }

    #[test]
    fn package_names() {
        assert_eq!(
            declared_package("/* x\n package Nope */\nstandard library package ISQ {").as_deref(),
            Some("ISQ")
        );
        assert_eq!(
            declared_package("package 'Quantities and Units' {").as_deref(),
            Some("Quantities and Units")
        );
        assert_eq!(declared_package("part def P;"), None);
    }

    #[test]
    fn search_ranks_exact_then_prefix() {
        if !embedded() {
            return;
        }
        let hits = search("port", 50);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].name.to_lowercase(), "port");
        let first_contains = hits
            .iter()
            .position(|h| !h.name.to_lowercase().starts_with("port"))
            .unwrap_or(hits.len());
        assert!(hits[first_contains..]
            .iter()
            .all(|h| !h.name.to_lowercase().starts_with("port")));
        let port_def = search("Port", 10)
            .into_iter()
            .find(|h| h.is_definition && h.path == "Systems Library/Ports.sysml")
            .expect("port def Port");
        assert_eq!(port_def.qualified_name, "Ports::Port");
        assert_eq!(port_def.kind, "port def");
        assert!(port_def.line > 1);
        assert!(port_def.doc.is_some());
        assert_eq!(search("port", 3).len(), 3);
        assert!(search("  ", 10).is_empty());
    }

    #[test]
    fn search_by_qualified_name() {
        if !embedded() {
            return;
        }
        let hits = search("ScalarValues::Real", 5);
        assert!(hits
            .iter()
            .any(|h| h.qualified_name == "ScalarValues::Real"));
    }

    #[test]
    fn license_is_lgpl() {
        let l = license();
        assert_eq!(l.spdx, "LGPL-3.0-only");
        assert!(l.notice.contains("Lesser General Public License"));
    }
}
