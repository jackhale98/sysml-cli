use sysml_core::model::Model;
use tower_lsp_server::ls_types::{FoldingRange, FoldingRangeKind};

/// Compute folding ranges from model definitions that have a body block.
/// Each definition with `has_body == true` gets a folding range from its
/// opening `{` line to its closing `}` line.
pub fn folding_ranges(model: &Model) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();

    for def in &model.definitions {
        if def.has_body && def.span.end_row > def.span.start_row {
            ranges.push(FoldingRange {
                start_line: (def.span.start_row - 1) as u32,
                start_character: None,
                end_line: (def.span.end_row - 1) as u32,
                end_character: None,
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: Some(format!("{} {} {{ ... }}", def.kind.label(), def.name)),
            });
        }
    }

    // Also fold doc comments that span multiple lines
    for comment in &model.comments {
        if comment.span.end_row > comment.span.start_row {
            ranges.push(FoldingRange {
                start_line: (comment.span.start_row - 1) as u32,
                start_character: None,
                end_line: (comment.span.end_row - 1) as u32,
                end_character: None,
                kind: Some(FoldingRangeKind::Comment),
                collapsed_text: None,
            });
        }
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysml_core::parser::parse_file;

    #[test]
    fn folds_definition_with_body() {
        let source = "part def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let folds = folding_ranges(&model);
        assert_eq!(folds.len(), 1);
        assert_eq!(folds[0].start_line, 0); // line 0 (0-based)
        assert_eq!(folds[0].end_line, 2); // closing }
        assert_eq!(folds[0].kind, Some(FoldingRangeKind::Region));
    }

    #[test]
    fn no_fold_for_semicolon_def() {
        let source = "part def Vehicle;\n";
        let model = parse_file("test.sysml", source);
        let folds = folding_ranges(&model);
        assert!(folds.is_empty(), "semicolon def should not fold");
    }

    #[test]
    fn nested_folds() {
        let source =
            "package P {\n    part def Vehicle {\n        part engine : Engine;\n    }\n}\n";
        let model = parse_file("test.sysml", source);
        let folds = folding_ranges(&model);
        assert!(
            folds.len() >= 2,
            "should have folds for package and part def"
        );
    }

    #[test]
    fn collapsed_text_shows_def_kind_and_name() {
        let source = "part def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let folds = folding_ranges(&model);
        assert_eq!(
            folds[0].collapsed_text.as_deref(),
            Some("part def Vehicle { ... }")
        );
    }

    #[test]
    fn empty_file_no_folds() {
        let model = parse_file("test.sysml", "");
        let folds = folding_ranges(&model);
        assert!(folds.is_empty());
    }
}
