use tower_lsp::lsp_types::{
    self, CodeAction, CodeActionKind, NumberOrString, Position, Range, TextEdit, Url, WorkspaceEdit,
};

use std::collections::HashMap;

/// Extract quick-fix code actions from diagnostics in the given range.
/// `workspace_names` enables "add import" suggestions for unresolved types.
pub fn code_actions(
    uri: &Url,
    diagnostics: &[lsp_types::Diagnostic],
    source: Option<&str>,
    workspace_names: Option<&[String]>,
) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    for diag in diagnostics {
        // "Did you mean" quick-fix for W004 (unresolved type)
        if let Some(ref suggestion) = extract_did_you_mean(&diag.message) {
            let mut changes = HashMap::new();
            changes.insert(
                uri.clone(),
                vec![TextEdit {
                    range: diag.range,
                    new_text: suggestion.clone(),
                }],
            );

            actions.push(CodeAction {
                title: format!("Replace with `{}`", suggestion),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                is_preferred: Some(true),
                ..Default::default()
            });
        }

        // "Remove unused definition" for W001
        if diag.code == Some(NumberOrString::String("W001".to_string())) {
            if let Some(source) = source {
                let delete_range = expand_range_to_full_lines(source, &diag.range);
                let mut changes = HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit {
                        range: delete_range,
                        new_text: String::new(),
                    }],
                );

                let name = extract_backtick_name(&diag.message).unwrap_or("definition");
                actions.push(CodeAction {
                    title: format!("Remove unused `{}`", name),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }),
                    is_preferred: Some(false),
                    ..Default::default()
                });
            }
        }

        // "Add import" for W004 (unresolved type) when the type exists in workspace
        if diag.code == Some(NumberOrString::String("W004".to_string())) {
            if let Some(ws_names) = workspace_names {
                if let Some(type_name) = extract_backtick_name(&diag.message) {
                    if ws_names.iter().any(|n| n == type_name) {
                        if let Some(source) = source {
                            // Insert import at the top of the file (after any existing imports)
                            let insert_line = find_import_insert_line(source);
                            let import_text = format!("import {}::*;\n", type_name);
                            let mut changes = HashMap::new();
                            changes.insert(
                                uri.clone(),
                                vec![TextEdit {
                                    range: Range::new(
                                        Position::new(insert_line, 0),
                                        Position::new(insert_line, 0),
                                    ),
                                    new_text: import_text,
                                }],
                            );
                            actions.push(CodeAction {
                                title: format!("Add import for `{}`", type_name),
                                kind: Some(CodeActionKind::QUICKFIX),
                                diagnostics: Some(vec![diag.clone()]),
                                edit: Some(WorkspaceEdit {
                                    changes: Some(changes),
                                    ..Default::default()
                                }),
                                is_preferred: Some(false),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
    }

    actions
}

/// Find the line number to insert a new import statement.
fn find_import_insert_line(source: &str) -> u32 {
    let mut last_import_line = 0u32;
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") || trimmed.starts_with("private import ") {
            last_import_line = (i + 1) as u32;
        }
    }
    last_import_line
}

/// Parse a "did you mean `X`?" suggestion from a diagnostic message.
fn extract_did_you_mean(message: &str) -> Option<String> {
    let marker = "did you mean `";
    let start = message.find(marker)? + marker.len();
    let rest = &message[start..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

/// Extract the first `backtick-quoted` name from a message.
fn extract_backtick_name(message: &str) -> Option<&str> {
    let start = message.find('`')? + 1;
    let rest = &message[start..];
    let end = rest.find('`')?;
    Some(&rest[..end])
}

/// Expand a range to cover the full lines it spans (including trailing newline).
fn expand_range_to_full_lines(source: &str, range: &Range) -> Range {
    let lines: Vec<&str> = source.lines().collect();
    let start_line = range.start.line as usize;
    let end_line = range.end.line as usize;

    // End position: start of the next line (to consume the newline)
    let next_line = end_line + 1;
    let (final_line, final_col) = if next_line <= lines.len() {
        (next_line as u32, 0)
    } else {
        // Last line with no trailing newline
        let last_col = lines.get(end_line).map(|l| l.len()).unwrap_or(0);
        (end_line as u32, last_col as u32)
    };

    Range::new(
        Position::new(start_line as u32, 0),
        Position::new(final_line, final_col),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::DiagnosticSeverity;

    fn make_diag(
        code: &str,
        message: &str,
        line: u32,
        start_col: u32,
        end_col: u32,
    ) -> lsp_types::Diagnostic {
        lsp_types::Diagnostic {
            range: Range::new(Position::new(line, start_col), Position::new(line, end_col)),
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String(code.to_string())),
            source: Some("sysml".to_string()),
            message: message.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn extracts_did_you_mean() {
        assert_eq!(
            extract_did_you_mean(
                "type `Vehicel` is not defined\nSuggestion: did you mean `Vehicle`?"
            ),
            Some("Vehicle".to_string())
        );
    }

    #[test]
    fn no_suggestion_returns_none() {
        assert_eq!(
            extract_did_you_mean("type `Unknown` is not defined in this file"),
            None
        );
    }

    #[test]
    fn extracts_backtick_name() {
        assert_eq!(
            extract_backtick_name("part def `Vehicle` is unused"),
            Some("Vehicle")
        );
        assert_eq!(extract_backtick_name("no backticks here"), None);
    }

    #[test]
    fn quickfix_from_unresolved_type() {
        let uri = Url::parse("file:///test.sysml").unwrap();
        let diags = vec![make_diag(
            "W004",
            "type `Vehicel` is not defined\nSuggestion: did you mean `Vehicle`?",
            1,
            15,
            22,
        )];
        let actions = code_actions(&uri, &diags, None, None);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Replace with `Vehicle`");
        assert_eq!(actions[0].kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(actions[0].is_preferred, Some(true));
    }

    #[test]
    fn remove_unused_definition() {
        let source = "part def Unused;\npart def Used;\n";
        let uri = Url::parse("file:///test.sysml").unwrap();
        let diag = lsp_types::Diagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 16)),
            severity: Some(DiagnosticSeverity::INFORMATION),
            code: Some(NumberOrString::String("W001".to_string())),
            source: Some("sysml".to_string()),
            message: "part def `Unused` is defined but never referenced".to_string(),
            ..Default::default()
        };
        let actions = code_actions(&uri, &[diag], Some(source), None);
        let remove: Vec<_> = actions
            .iter()
            .filter(|a| a.title.contains("Remove"))
            .collect();
        assert_eq!(remove.len(), 1);
        assert!(remove[0].title.contains("Unused"));
        // The edit should delete the entire first line
        let edit = remove[0].edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let edits = changes.get(&uri).unwrap();
        assert_eq!(edits[0].new_text, "");
        assert_eq!(edits[0].range.start.line, 0);
        assert_eq!(edits[0].range.start.character, 0);
        assert_eq!(edits[0].range.end.line, 1); // consumes the newline
    }

    #[test]
    fn no_actions_for_diag_without_suggestion() {
        let uri = Url::parse("file:///test.sysml").unwrap();
        let diags = vec![make_diag("E002", "duplicate definition `A`", 0, 0, 10)];
        let actions = code_actions(&uri, &diags, None, None);
        assert!(actions.is_empty());
    }

    #[test]
    fn multiple_suggestions_multiple_actions() {
        let uri = Url::parse("file:///test.sysml").unwrap();
        let diags = vec![
            make_diag("W004", "Suggestion: did you mean `Vehicle`?", 1, 15, 22),
            make_diag("W004", "Suggestion: did you mean `Engine`?", 2, 15, 21),
        ];
        let actions = code_actions(&uri, &diags, None, None);
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn integration_with_real_diagnostics() {
        use crate::diagnostics::compute_diagnostics;
        use sysml_core::parser::parse_file;

        let source = "part def Vehicle;\npart car : Vehicel;\n";
        let model = parse_file("test.sysml", source);
        let diags = compute_diagnostics(&model, &[]);

        let uri = Url::parse("file:///test.sysml").unwrap();
        let actions = code_actions(&uri, &diags, Some(source), None);

        let fixes: Vec<_> = actions
            .iter()
            .filter(|a| a.kind == Some(CodeActionKind::QUICKFIX))
            .collect();
        assert!(!fixes.is_empty());
    }

    #[test]
    fn expand_range_to_full_lines_works() {
        let source = "line one\nline two\nline three\n";
        let range = Range::new(Position::new(1, 2), Position::new(1, 8));
        let expanded = expand_range_to_full_lines(source, &range);
        assert_eq!(expanded.start.line, 1);
        assert_eq!(expanded.start.character, 0);
        assert_eq!(expanded.end.line, 2);
        assert_eq!(expanded.end.character, 0);
    }
}
