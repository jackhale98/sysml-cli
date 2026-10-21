//! Hover: the markdown comes from `sysml_core::lang::hover`.

pub use sysml_core::lang::hover::{hover_info, hover_usage_info};

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn hover_def_with_doc_and_members() {
        let source =
            "part def Vehicle {\n    doc /* A motor vehicle. */\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let info = hover_info(&model, "Vehicle");
        assert!(info.is_some());
        let text = info.unwrap();
        assert!(text.contains("**part def**"), "should show kind: {}", text);
        assert!(text.contains("`Vehicle`"), "should show name: {}", text);
        assert!(
            text.contains("A motor vehicle"),
            "should show doc: {}",
            text
        );
        assert!(text.contains("`engine`"), "should show member: {}", text);
        assert!(
            text.contains("`Engine`"),
            "should show member type: {}",
            text
        );
    }

    #[test]
    fn hover_def_with_supertype() {
        let source = "part def Base;\npart def Sub :> Base;\n";
        let model = parse_file("test.sysml", source);
        let info = hover_info(&model, "Sub");
        assert!(info.is_some());
        let text = info.unwrap();
        assert!(text.contains(": `Base`"), "should show supertype: {}", text);
    }

    #[test]
    fn hover_usage() {
        let source = "part def Engine;\npart def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let info = hover_usage_info(&model, "engine");
        assert!(info.is_some());
        let text = info.unwrap();
        assert!(text.contains("**part**"), "should show kind: {}", text);
        assert!(text.contains("`Engine`"), "should show type ref: {}", text);
        assert!(
            text.contains("In `Vehicle`"),
            "should show parent: {}",
            text
        );
    }

    #[test]
    fn hover_unknown_returns_none() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        assert!(hover_info(&model, "Unknown").is_none());
    }

    #[test]
    fn hover_shows_rollup() {
        let source = r#"
            part def Engine { attribute mass : Real = 180; }
            part def Vehicle {
                attribute mass : Real = 50;
                part engine : Engine;
            }
        "#;
        let model = parse_file("test.sysml", source);
        let info = hover_info(&model, "Vehicle");
        assert!(info.is_some());
        let text = info.unwrap();
        assert!(
            text.contains("Rollups:"),
            "should show rollup section: {}",
            text
        );
        assert!(
            text.contains("230"),
            "should show total 230 (50+180): {}",
            text
        );
    }
}
