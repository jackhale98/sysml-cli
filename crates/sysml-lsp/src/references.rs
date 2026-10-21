//! References within one model; the search lives in
//! `sysml_core::lang::references`.

pub use sysml_core::lang::references::find_references_in_model;

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::lang::references::find_all_references;
    use sysml_core::model::Model;
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
