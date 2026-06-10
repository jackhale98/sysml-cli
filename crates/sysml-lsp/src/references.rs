use sysml_core::model::{simple_name, Model, Span};

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

    // Definition super_types referencing this name
    for def in &model.definitions {
        if let Some(ref st) = def.super_type {
            if simple_name(st) == target {
                refs.push(ReferenceLocation {
                    uri: uri.to_string(),
                    span: def.span.clone(),
                });
            }
        }
    }

    // Usage type_refs
    for usage in &model.usages {
        if let Some(ref tr) = usage.type_ref {
            if simple_name(tr) == target {
                refs.push(ReferenceLocation {
                    uri: uri.to_string(),
                    span: usage.span.clone(),
                });
            }
        }
    }

    // Explicit type_references
    for tr in &model.type_references {
        if simple_name(&tr.name) == target {
            refs.push(ReferenceLocation {
                uri: uri.to_string(),
                span: tr.span.clone(),
            });
        }
    }

    // Connections
    for conn in &model.connections {
        if simple_name(&conn.source) == target || simple_name(&conn.target) == target {
            refs.push(ReferenceLocation {
                uri: uri.to_string(),
                span: conn.span.clone(),
            });
        }
    }

    // Flows
    for flow in &model.flows {
        if simple_name(&flow.source) == target
            || simple_name(&flow.target) == target
            || flow.item_type.as_deref().map(simple_name) == Some(target)
        {
            refs.push(ReferenceLocation {
                uri: uri.to_string(),
                span: flow.span.clone(),
            });
        }
    }

    // Satisfactions
    for sat in &model.satisfactions {
        if simple_name(&sat.requirement) == target
            || sat.by.as_deref().map(simple_name) == Some(target)
        {
            refs.push(ReferenceLocation {
                uri: uri.to_string(),
                span: sat.span.clone(),
            });
        }
    }

    // Verifications
    for ver in &model.verifications {
        if simple_name(&ver.requirement) == target || simple_name(&ver.by) == target {
            refs.push(ReferenceLocation {
                uri: uri.to_string(),
                span: ver.span.clone(),
            });
        }
    }

    // Allocations
    for alloc in &model.allocations {
        if simple_name(&alloc.source) == target || simple_name(&alloc.target) == target {
            refs.push(ReferenceLocation {
                uri: uri.to_string(),
                span: alloc.span.clone(),
            });
        }
    }

    refs
}

#[allow(dead_code)] // used in tests; will be wired into server when needed
/// Find all references to `name` across multiple models.
/// If `include_declaration` is true, also includes the definition site itself.
pub fn find_all_references(
    models: &[(&str, &Model)],
    name: &str,
    include_declaration: bool,
) -> Vec<ReferenceLocation> {
    let target = simple_name(name);
    let mut all_refs = Vec::new();

    for (uri, model) in models {
        // Optionally include the declaration itself
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
    use sysml_core::parser::parse_file;

    #[test]
    fn finds_type_ref_in_usage() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let refs = find_references_in_model(&model, "file:///test.sysml", "Engine");
        // Should find at least the type reference in `part engine : Engine`
        assert!(!refs.is_empty(), "expected references to Engine");
    }

    #[test]
    fn finds_supertype_reference() {
        let source = "part def Base;\npart def Sub :> Base;\n";
        let model = parse_file("test.sysml", source);
        let refs = find_references_in_model(&model, "file:///test.sysml", "Base");
        assert!(!refs.is_empty(), "expected supertype reference to Base");
    }

    #[test]
    fn no_references_for_unknown() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let refs = find_references_in_model(&model, "file:///test.sysml", "Unknown");
        assert!(refs.is_empty());
    }

    #[test]
    fn cross_file_references() {
        let source_a = "part def Engine;\n";
        let source_b = "part def Vehicle {\n    part engine : Engine;\n}\n";
        let model_a = parse_file("a.sysml", source_a);
        let model_b = parse_file("b.sysml", source_b);

        let models: Vec<(&str, &Model)> =
            vec![("file:///a.sysml", &model_a), ("file:///b.sysml", &model_b)];
        let refs = find_all_references(&models, "Engine", false);
        // Should find reference in b.sysml
        assert!(refs.iter().any(|r| r.uri == "file:///b.sysml"));
    }

    #[test]
    fn include_declaration_flag() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let models: Vec<(&str, &Model)> = vec![("file:///test.sysml", &model)];

        let without = find_all_references(&models, "Engine", false);
        let with = find_all_references(&models, "Engine", true);
        // Including declaration should give at least one more result
        assert!(
            with.len() > without.len(),
            "with declaration ({}) should be > without ({})",
            with.len(),
            without.len()
        );
    }

    #[test]
    fn multiple_references_in_one_file() {
        let source =
            "part def Engine;\npart def Car {\n    part e1 : Engine;\n    part e2 : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let refs = find_references_in_model(&model, "file:///test.sysml", "Engine");
        // At least 2 usages referencing Engine
        assert!(
            refs.len() >= 2,
            "expected at least 2 references, got {}",
            refs.len()
        );
    }
}
