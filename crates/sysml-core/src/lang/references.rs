//! Find references.
//!
//! [`references`] finds, across the project, every name that resolves to
//! the same element as the name under the cursor (declaration included);
//! [`find_references_in_model`] is the per-model search of the language
//! server.

use super::index::{Src, Workspace};
use super::Location;
use crate::model::{simple_name, Model, Span};

/// Every occurrence of the element named at byte `offset` of `src`, in
/// file order.
pub fn references(ws: &Workspace, src: Src, offset: usize) -> Vec<Location> {
    let Some((id, _)) = ws.resolve_at(src, offset) else {
        return Vec::new();
    };
    ws.occurrences(id)
        .into_iter()
        .map(|(s, start, end)| Location {
            path: ws.file(s).path.clone(),
            start,
            end,
        })
        .collect()
}

/// A reference location: the URI of the file and the span within it.
#[derive(Debug, Clone)]
pub struct ReferenceLocation {
    pub uri: String,
    pub span: Span,
}

/// Find all references to `name` within a single model (by URI).
/// Searches definitions (super_type), usages (type_ref), type_references,
/// connections, flows, satisfactions, verifications, and allocations.
pub fn find_references_in_model(model: &Model, uri: &str, name: &str) -> Vec<ReferenceLocation> {
    let target = simple_name(name);
    let mut refs = Vec::new();
    let mut push = |span: &Span| {
        refs.push(ReferenceLocation {
            uri: uri.to_string(),
            span: span.clone(),
        })
    };

    for def in &model.definitions {
        if def.super_type.as_deref().map(simple_name) == Some(target) {
            push(&def.span);
        }
    }
    for usage in &model.usages {
        if usage.type_ref.as_deref().map(simple_name) == Some(target) {
            push(&usage.span);
        }
    }
    for tr in &model.type_references {
        if simple_name(&tr.name) == target {
            push(&tr.span);
        }
    }
    for conn in &model.connections {
        if simple_name(&conn.source) == target || simple_name(&conn.target) == target {
            push(&conn.span);
        }
    }
    for flow in &model.flows {
        if simple_name(&flow.source) == target
            || simple_name(&flow.target) == target
            || flow.item_type.as_deref().map(simple_name) == Some(target)
        {
            push(&flow.span);
        }
    }
    for sat in &model.satisfactions {
        if simple_name(&sat.requirement) == target
            || sat.by.as_deref().map(simple_name) == Some(target)
        {
            push(&sat.span);
        }
    }
    for ver in &model.verifications {
        if simple_name(&ver.requirement) == target || simple_name(&ver.by) == target {
            push(&ver.span);
        }
    }
    for alloc in &model.allocations {
        if simple_name(&alloc.source) == target || simple_name(&alloc.target) == target {
            push(&alloc.span);
        }
    }
    refs
}

/// Find all references to `name` across multiple models. With
/// `include_declaration`, the definition site is included too.
pub fn find_all_references(
    models: &[(&str, &Model)],
    name: &str,
    include_declaration: bool,
) -> Vec<ReferenceLocation> {
    let target = simple_name(name);
    let mut all_refs = Vec::new();
    for (uri, model) in models {
        if include_declaration {
            if let Some(def) = model.find_def(target) {
                all_refs.push(ReferenceLocation {
                    uri: uri.to_string(),
                    span: def.span.clone(),
                });
            }
        }
        all_refs.extend(find_references_in_model(model, uri, name));
    }
    all_refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    #[test]
    fn model_references_across_files() {
        let model_a = parse_file("a.sysml", "part def Engine;\n");
        let model_b = parse_file(
            "b.sysml",
            "part def Vehicle {\n    part engine : Engine;\n}\n",
        );
        let models: Vec<(&str, &Model)> =
            vec![("file:///a.sysml", &model_a), ("file:///b.sysml", &model_b)];
        let refs = find_all_references(&models, "Engine", false);
        assert!(refs.iter().any(|r| r.uri == "file:///b.sysml"));
        assert!(find_all_references(&models, "Engine", true).len() > refs.len());
    }

    #[test]
    fn project_references_are_exact_and_scoped() {
        let a = "package A {\n    private import B::*;\n    part d { part battery : Battery; }\n    // Battery in a comment\n    part def Other { part battery : 'Battery pack'; }\n}\n";
        let b = "package B {\n    part def Battery;\n    part def 'Battery pack';\n}\n";
        let ws = Workspace::from_sources([("a.sysml", a), ("b.sysml", b)]);
        let refs = references(&ws, Src::Project(1), b.find("Battery").unwrap());
        let found: Vec<(&str, &str)> = refs
            .iter()
            .map(|l| {
                let text = if l.path == "a.sysml" { a } else { b };
                (l.path.as_str(), &text[l.start..l.end])
            })
            .collect();
        assert_eq!(found, [("a.sysml", "Battery"), ("b.sysml", "Battery")]);

        // The two `battery` usages are different elements.
        let usage = references(&ws, Src::Project(0), a.find("battery").unwrap());
        assert_eq!(usage.len(), 1);
    }
}
