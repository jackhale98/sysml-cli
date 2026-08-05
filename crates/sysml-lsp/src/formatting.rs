use tower_lsp_server::ls_types::{FormattingOptions, Position, Range, TextEdit};

use sysml_core::codegen::format::{format_source, FormatOptions};

/// Format a SysML source using editor-provided options.
/// Returns None if the source is already correctly formatted.
pub fn format_document(
    source: &str,
    editor_opts: Option<&FormattingOptions>,
) -> Option<Vec<TextEdit>> {
    let opts = if let Some(eo) = editor_opts {
        FormatOptions {
            indent_width: eo.tab_size as usize,
            trailing_newline: eo.insert_final_newline.unwrap_or(true),
        }
    } else {
        FormatOptions::default()
    };

    let formatted = format_source(source, &opts);

    if formatted == source {
        return None;
    }

    let line_count = source.lines().count();
    let last_line = if line_count == 0 { 0 } else { line_count - 1 };
    let last_col = source.lines().last().map(|l| l.len()).unwrap_or(0);

    let (end_line, end_col) = if source.ends_with('\n') {
        (last_line + 1, 0)
    } else {
        (last_line, last_col)
    };

    Some(vec![TextEdit {
        range: Range::new(
            Position::new(0, 0),
            Position::new(end_line as u32, end_col as u32),
        ),
        new_text: formatted,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_misindented_source() {
        let source = "part def Vehicle {\npart engine : Engine;\n}\n";
        let edits = format_document(source, None);
        assert!(edits.is_some());
        assert!(edits.unwrap()[0].new_text.contains("    part engine"));
    }

    #[test]
    fn already_formatted_returns_none() {
        let source = "part def Vehicle {\n    part engine : Engine;\n}\n";
        assert!(format_document(source, None).is_none());
    }

    #[test]
    fn formats_nested_definitions() {
        let source = "package P {\npart def Vehicle {\npart engine : Engine;\n}\n}\n";
        let edits = format_document(source, None).unwrap();
        let formatted = &edits[0].new_text;
        assert!(formatted.contains("    part def Vehicle {"));
        assert!(formatted.contains("        part engine : Engine;"));
    }

    #[test]
    fn edit_covers_entire_document() {
        let source = "part def Vehicle {\npart engine : Engine;\n}\n";
        let edits = format_document(source, None).unwrap();
        assert_eq!(edits[0].range.start.line, 0);
        assert_eq!(edits[0].range.start.character, 0);
    }

    #[test]
    fn empty_source() {
        let source = "";
        let edits = format_document(source, None);
        if let Some(edits) = edits {
            assert_eq!(edits[0].new_text, "\n");
        }
    }

    #[test]
    fn preserves_doc_comments() {
        let source = "part def Vehicle {\ndoc /* A vehicle */\npart engine : Engine;\n}\n";
        let formatted = &format_document(source, None).unwrap()[0].new_text;
        assert!(formatted.contains("doc /* A vehicle */"));
    }

    #[test]
    fn respects_editor_tab_size() {
        let source = "part def Vehicle {\npart engine : Engine;\n}\n";
        let editor_opts = FormattingOptions {
            tab_size: 2,
            insert_spaces: true,
            ..Default::default()
        };
        let edits = format_document(source, Some(&editor_opts)).unwrap();
        let formatted = &edits[0].new_text;
        assert!(
            formatted.contains("  part engine"),
            "should use 2-space indent, got: {}",
            formatted
        );
        assert!(
            !formatted.contains("    part engine"),
            "should NOT use 4-space indent"
        );
    }

    #[test]
    fn default_is_4_spaces() {
        let source = "part def Vehicle {\npart engine : Engine;\n}\n";
        let edits = format_document(source, None).unwrap();
        assert!(edits[0].new_text.contains("    part engine"));
    }
}
