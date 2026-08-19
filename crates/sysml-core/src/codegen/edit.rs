//! Byte-position text edits for surgical modification of SysML v2 source files.

use crate::model::Model;

/// A single text edit expressed as a byte range replacement.
#[derive(Debug, Clone)]
pub struct TextEdit {
    /// Start byte offset in the original source.
    pub start_byte: usize,
    /// End byte offset (exclusive) in the original source.
    /// For pure insertions, `start_byte == end_byte`.
    pub end_byte: usize,
    /// Replacement text.
    pub new_text: String,
}

/// A collection of edits to apply atomically.
#[derive(Debug, Clone, Default)]
pub struct EditPlan {
    pub edits: Vec<TextEdit>,
}

/// Error type for edit operations.
#[derive(Debug, Clone)]
pub struct EditError {
    pub message: String,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl EditPlan {
    pub fn new() -> Self {
        Self { edits: Vec::new() }
    }

    pub fn add(&mut self, edit: TextEdit) {
        self.edits.push(edit);
    }
}

/// Apply a set of edits to source text.
///
/// Edits are applied back-to-front (highest byte offset first) so that
/// earlier edits do not invalidate the byte positions of later ones.
/// Returns an error if edits overlap.
pub fn apply_edits(source: &str, plan: &EditPlan) -> Result<String, EditError> {
    let mut edits = plan.edits.clone();
    // Sort by start_byte descending so we apply from end to start
    edits.sort_by_key(|e| std::cmp::Reverse(e.start_byte));

    // Validate no overlaps
    for pair in edits.windows(2) {
        // pair[0] has higher start_byte, pair[1] has lower
        if pair[1].end_byte > pair[0].start_byte {
            return Err(EditError {
                message: format!(
                    "overlapping edits: [{}, {}) and [{}, {})",
                    pair[1].start_byte, pair[1].end_byte, pair[0].start_byte, pair[0].end_byte,
                ),
            });
        }
    }

    let mut result = source.to_string();
    for edit in &edits {
        if edit.start_byte > result.len() || edit.end_byte > result.len() {
            return Err(EditError {
                message: format!(
                    "edit out of bounds: [{}, {}) in source of {} bytes",
                    edit.start_byte,
                    edit.end_byte,
                    result.len()
                ),
            });
        }
        result.replace_range(edit.start_byte..edit.end_byte, &edit.new_text);
    }

    Ok(result)
}

/// Create an edit that inserts text inside a definition body (before the closing `}`).
///
/// The text is indented to match the body's nesting level.
pub fn insert_member(
    source: &str,
    model: &Model,
    parent_name: &str,
    member_text: &str,
) -> Result<TextEdit, EditError> {
    let def = model.find_def(parent_name).ok_or_else(|| EditError {
        message: format!("definition `{}` not found", parent_name),
    })?;

    let close_byte = def.body_end_byte.ok_or_else(|| EditError {
        message: format!("definition `{}` has no body (no closing `}}`)", parent_name),
    })?;

    // Determine indentation: look at the column of the definition's opening line
    // and add one indent level (4 spaces)
    let indent = detect_indent(source, def.span.start_col);

    // Build the insertion text: newline + indented member + newline
    let indented = member_text
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("{}{}", indent, line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Check if there's content before the closing brace on its line
    let before_close = &source[..close_byte];
    let needs_newline = !before_close.ends_with('\n');

    let insert_text = if needs_newline {
        format!("\n{}\n", indented)
    } else {
        format!("{}\n", indented)
    };

    Ok(TextEdit {
        start_byte: close_byte,
        end_byte: close_byte,
        new_text: insert_text,
    })
}

/// Create an edit that appends text at the end of the file.
pub fn insert_top_level(source: &str, text: &str) -> TextEdit {
    let insert_pos = source.len();
    let separator = if source.ends_with('\n') { "\n" } else { "\n\n" };
    TextEdit {
        start_byte: insert_pos,
        end_byte: insert_pos,
        new_text: format!("{}{}\n", separator, text),
    }
}

/// Create an edit that removes an element by name.
pub fn remove_element(
    source: &str,
    model: &Model,
    element_name: &str,
) -> Result<TextEdit, EditError> {
    // Try definitions first, then usages
    if let Some(def) = model.find_def(element_name) {
        let (start, end) = expand_to_full_line(source, def.span.start_byte, def.span.end_byte);
        return Ok(TextEdit {
            start_byte: start,
            end_byte: end,
            new_text: String::new(),
        });
    }

    if let Some(usage) = model.usages.iter().find(|u| u.name == element_name) {
        let (start, end) = expand_to_full_line(source, usage.span.start_byte, usage.span.end_byte);
        return Ok(TextEdit {
            start_byte: start,
            end_byte: end,
            new_text: String::new(),
        });
    }

    Err(EditError {
        message: format!("element `{}` not found", element_name),
    })
}

/// Create edits that rename an element and all references to it.
pub fn rename_element(
    source: &str,
    _model: &Model,
    old_name: &str,
    new_name: &str,
) -> Result<EditPlan, EditError> {
    let mut plan = EditPlan::new();

    // Find all byte positions where old_name appears as a whole word
    let bytes = source.as_bytes();
    let old_bytes = old_name.as_bytes();
    let mut pos = 0;
    while pos + old_bytes.len() <= bytes.len() {
        if &bytes[pos..pos + old_bytes.len()] == old_bytes {
            // Check word boundaries
            let before_ok = pos == 0 || !is_ident_char(bytes[pos - 1]);
            let after_ok = pos + old_bytes.len() >= bytes.len()
                || !is_ident_char(bytes[pos + old_bytes.len()]);
            if before_ok && after_ok {
                plan.add(TextEdit {
                    start_byte: pos,
                    end_byte: pos + old_bytes.len(),
                    new_text: new_name.to_string(),
                });
            }
        }
        pos += 1;
    }

    if plan.edits.is_empty() {
        return Err(EditError {
            message: format!("no occurrences of `{}` found", old_name),
        });
    }

    Ok(plan)
}

/// Generate a unified diff between original and modified source.
pub fn diff(original: &str, modified: &str, filename: &str) -> String {
    let mut output = String::new();
    output.push_str(&format!("--- a/{}\n", filename));
    output.push_str(&format!("+++ b/{}\n", filename));

    let orig_lines: Vec<&str> = original.lines().collect();
    let mod_lines: Vec<&str> = modified.lines().collect();

    // Simple line-by-line diff (not optimal but functional)
    let mut i = 0;
    let mut j = 0;
    while i < orig_lines.len() || j < mod_lines.len() {
        if i < orig_lines.len() && j < mod_lines.len() && orig_lines[i] == mod_lines[j] {
            i += 1;
            j += 1;
        } else {
            // Find the extent of the change
            let start_i = i;
            let start_j = j;
            // Advance until lines match again
            let mut found = false;
            for di in 0..=5 {
                for dj in 0..=5 {
                    if i + di < orig_lines.len()
                        && j + dj < mod_lines.len()
                        && orig_lines[i + di] == mod_lines[j + dj]
                        && (di > 0 || dj > 0)
                    {
                        // Output the hunk
                        output.push_str(&format!(
                            "@@ -{},{} +{},{} @@\n",
                            start_i + 1,
                            di,
                            start_j + 1,
                            dj
                        ));
                        for line in &orig_lines[start_i..i + di] {
                            output.push_str(&format!("-{}\n", line));
                        }
                        for line in &mod_lines[start_j..j + dj] {
                            output.push_str(&format!("+{}\n", line));
                        }
                        i += di;
                        j += dj;
                        found = true;
                        break;
                    }
                }
                if found {
                    break;
                }
            }
            if !found {
                // Remaining lines differ
                output.push_str(&format!(
                    "@@ -{},{} +{},{} @@\n",
                    start_i + 1,
                    orig_lines.len() - start_i,
                    start_j + 1,
                    mod_lines.len() - start_j,
                ));
                while i < orig_lines.len() {
                    output.push_str(&format!("-{}\n", orig_lines[i]));
                    i += 1;
                }
                while j < mod_lines.len() {
                    output.push_str(&format!("+{}\n", mod_lines[j]));
                    j += 1;
                }
            }
        }
    }

    output
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Expand a byte range to include the full line(s) and trailing newline.
fn expand_to_full_line(source: &str, start: usize, end: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    // Find start of line
    let mut line_start = start;
    while line_start > 0 && bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }
    // Find end of line (include trailing newline)
    let mut line_end = end;
    while line_end < bytes.len() && bytes[line_end] != b'\n' {
        line_end += 1;
    }
    if line_end < bytes.len() {
        line_end += 1; // include the newline
    }
    (line_start, line_end)
}

/// Detect the indent string for children of a definition at the given column.
fn detect_indent(source: &str, parent_start_col: usize) -> String {
    // Parent's column is 1-based, convert to 0-based indent width
    let base_indent = parent_start_col.saturating_sub(1);
    // Detect if source uses tabs
    let uses_tabs = source.lines().any(|l| l.starts_with('\t'));
    if uses_tabs {
        format!("{}\t", "\t".repeat(base_indent / 4))
    } else {
        " ".repeat(base_indent + 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_simple_insertion() {
        let source = "hello world";
        let plan = EditPlan {
            edits: vec![TextEdit {
                start_byte: 5,
                end_byte: 5,
                new_text: " beautiful".to_string(),
            }],
        };
        let result = apply_edits(source, &plan).unwrap();
        assert_eq!(result, "hello beautiful world");
    }

    #[test]
    fn apply_replacement() {
        let source = "part def Vehicle;";
        let plan = EditPlan {
            edits: vec![TextEdit {
                start_byte: 9,
                end_byte: 16,
                new_text: "Car".to_string(),
            }],
        };
        let result = apply_edits(source, &plan).unwrap();
        assert_eq!(result, "part def Car;");
    }

    #[test]
    fn apply_multiple_edits() {
        let source = "part def A;\npart def B;";
        let plan = EditPlan {
            edits: vec![
                TextEdit {
                    start_byte: 9,
                    end_byte: 10,
                    new_text: "Alpha".to_string(),
                },
                TextEdit {
                    start_byte: 21,
                    end_byte: 22,
                    new_text: "Beta".to_string(),
                },
            ],
        };
        let result = apply_edits(source, &plan).unwrap();
        assert_eq!(result, "part def Alpha;\npart def Beta;");
    }

    #[test]
    fn overlapping_edits_rejected() {
        let source = "part def Vehicle;";
        let plan = EditPlan {
            edits: vec![
                TextEdit {
                    start_byte: 5,
                    end_byte: 10,
                    new_text: "x".to_string(),
                },
                TextEdit {
                    start_byte: 8,
                    end_byte: 12,
                    new_text: "y".to_string(),
                },
            ],
        };
        assert!(apply_edits(source, &plan).is_err());
    }

    #[test]
    fn insert_member_into_definition() {
        use crate::parser::parse_file;

        let source = "part def Vehicle {\n    part engine : Engine;\n}\n";
        let model = parse_file("test.sysml", source);
        let edit = insert_member(source, &model, "Vehicle", "part wheels : Wheel;").unwrap();
        let result = apply_edits(source, &EditPlan { edits: vec![edit] }).unwrap();
        assert!(result.contains("part wheels : Wheel;"));
        assert!(result.contains("part engine : Engine;"));
        // Should still have closing brace
        assert!(result.contains('}'));
    }

    #[test]
    fn remove_element_by_name() {
        use crate::parser::parse_file;

        let source = "part def Vehicle;\npart def Engine;\npart def Wheel;\n";
        let model = parse_file("test.sysml", source);
        let edit = remove_element(source, &model, "Engine").unwrap();
        let result = apply_edits(source, &EditPlan { edits: vec![edit] }).unwrap();
        assert!(result.contains("Vehicle"));
        assert!(!result.contains("Engine"));
        assert!(result.contains("Wheel"));
    }

    #[test]
    fn rename_element_all_occurrences() {
        let source = "part def Vehicle {\n    part engine : Engine;\n}\npart def Engine;\n";
        let model = crate::parser::parse_file("test.sysml", source);
        let plan = rename_element(source, &model, "Engine", "Motor").unwrap();
        let result = apply_edits(source, &plan).unwrap();
        assert!(!result.contains("Engine"));
        assert!(result.contains("Motor"));
        // Should rename both the definition and the type reference
        assert!(result.contains("part def Motor;"));
        assert!(result.contains(": Motor;"));
    }
}
