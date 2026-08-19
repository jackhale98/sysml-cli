//! Document-link provider for sysml-lsp.
//!
//! Returns clickable hyperlinks for:
//!   - `import` statement targets — link to the imported module file in
//!     the workspace (when located).
//!   - Qualified-name type references inside `:`, `:>`, etc. — link to
//!     the file that defines the trailing simple name.
//!
//! When a target cannot be resolved (e.g., refers to the stdlib at
//! parse-time), the entry is omitted rather than emitting a broken link.

use std::collections::HashMap;

use sysml_core::model::{simple_name, Model};
use tower_lsp_server::ls_types::{DocumentLink, Uri};

use crate::convert::span_to_range;

/// Build document links for a single file, using a workspace name → file
/// table (typically `Model::file` from each parsed workspace model).
pub fn document_links(
    model: &Model,
    source: &str,
    name_to_file: &HashMap<String, String>,
) -> Vec<DocumentLink> {
    let mut links = Vec::new();

    // Import statements
    for imp in &model.imports {
        let target_simple = simple_name(&imp.path);
        if let Some(file) = name_to_file.get(target_simple) {
            if let Some(url) = path_to_url(file) {
                links.push(DocumentLink {
                    range: span_to_range(&imp.span, source),
                    target: Some(url),
                    tooltip: Some(format!("Open definition of `{}`", target_simple)),
                    data: None,
                });
            }
        }
    }

    // Type references on usages (typed_by, conjugates) — use TypeReference list.
    for tr in &model.type_references {
        let target_simple = simple_name(&tr.name);
        if let Some(file) = name_to_file.get(target_simple) {
            if let Some(url) = path_to_url(file) {
                links.push(DocumentLink {
                    range: span_to_range(&tr.span, source),
                    target: Some(url),
                    tooltip: Some(format!("Open `{}`", target_simple)),
                    data: None,
                });
            }
        }
    }

    // Specialization super-types on definitions (`part def X :> Base;`).
    for def in &model.definitions {
        if let Some(ref super_ref) = def.super_type {
            let target_simple = simple_name(super_ref);
            if let Some(file) = name_to_file.get(target_simple) {
                if let Some(url) = path_to_url(file) {
                    links.push(DocumentLink {
                        range: span_to_range(&def.span, source),
                        target: Some(url),
                        tooltip: Some(format!("Open `{}`", target_simple)),
                        data: None,
                    });
                }
            }
        }
    }

    links
}

fn path_to_url(path: &str) -> Option<Uri> {
    if path.is_empty() {
        return None;
    }
    let absolute = std::path::Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(path));
    Uri::from_file_path(absolute)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    fn make_table(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn type_reference_with_known_file_produces_link() {
        let source = "part def Vehicle :> Base;\n";
        let model = parse_file("/tmp/main.sysml", source);
        // Pretend "Base" lives in /tmp/base.sysml (file need not exist for
        // the link generation logic).
        let table = make_table(&[("Base", "/tmp/base.sysml")]);
        let links = document_links(&model, source, &table);
        assert!(
            !links.is_empty(),
            "expected at least one document link, got {:?}",
            links
                .iter()
                .map(|l| l.target.as_ref().map(|u| u.as_str().to_string()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn unknown_target_produces_no_link() {
        let source = "part def Vehicle :> Unknown;\n";
        let model = parse_file("/tmp/main.sysml", source);
        let table: HashMap<String, String> = HashMap::new();
        let links = document_links(&model, source, &table);
        assert!(links.is_empty(), "unknown targets must not produce links");
    }

    #[test]
    fn import_statement_produces_link() {
        let source = r#"
            import Library::Vehicles;
            part def Car;
        "#;
        let model = parse_file("/tmp/main.sysml", source);
        let table = make_table(&[("Vehicles", "/tmp/lib.sysml")]);
        let links = document_links(&model, source, &table);
        assert!(
            !links.is_empty(),
            "import statement should produce a document link"
        );
    }
}
