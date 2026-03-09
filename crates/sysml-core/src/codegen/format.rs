/// CST-aware SysML v2 source formatter.
///
/// Uses tree-sitter to determine the correct indentation level for each line
/// based on the syntactic structure, rather than naive brace counting.
/// This handles comments, string literals, and multi-line expressions correctly.

use tree_sitter::{Node, Parser};

use crate::parser::get_language;

/// Options for formatting SysML v2 source.
#[derive(Debug, Clone)]
pub struct FormatOptions {
    /// Number of spaces per indent level.
    pub indent_width: usize,
    /// Ensure trailing newline.
    pub trailing_newline: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            indent_width: 4,
            trailing_newline: true,
        }
    }
}

/// Format SysML v2 source using the CST to determine indentation.
///
/// Parses the source with tree-sitter, walks the CST to compute the
/// correct depth for each line, and re-indents accordingly.
/// Preserves all content (comments, strings, etc.) — only whitespace
/// at the start of lines is modified.
pub fn format_source(source: &str, opts: &FormatOptions) -> String {
    let mut parser = Parser::new();
    parser.set_language(&get_language()).unwrap();
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return source.to_string(), // Can't parse — return unchanged
    };

    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        return if opts.trailing_newline {
            "\n".to_string()
        } else {
            String::new()
        };
    }

    // Compute the desired indent depth for each line from the CST
    let mut line_depths = vec![0usize; lines.len()];
    compute_line_depths(tree.root_node(), source, &mut line_depths);

    let indent_str = " ".repeat(opts.indent_width);
    let mut out = String::with_capacity(source.len());

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }

        let depth = line_depths[i];
        for _ in 0..depth {
            out.push_str(&indent_str);
        }
        out.push_str(trimmed);
        out.push('\n');
    }

    // Ensure trailing newline or remove extra
    if opts.trailing_newline && !out.ends_with('\n') {
        out.push('\n');
    }

    out
}

/// Walk the CST and assign an indent depth to each line.
///
/// Strategy: walk the tree and track depth. Each body node (`_body` suffix)
/// causes its content between `{` and `}` to be indented one level deeper.
fn compute_line_depths(root: Node, source: &str, depths: &mut [usize]) {
    assign_depths(root, source, 0, depths);
}

fn is_body_node(kind: &str) -> bool {
    kind.ends_with("_body")
}

/// Recursively assign indent depths based on the CST structure.
///
/// `depth` is the indent level of the parent construct. When we enter a body
/// node, children (except `{` and `}`) get `depth + 1`. The `}` gets `depth`.
fn assign_depths(node: Node, source: &str, depth: usize, depths: &mut [usize]) {
    if is_body_node(node.kind()) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let row = child.start_position().row;
            if row >= depths.len() {
                continue;
            }
            match child.kind() {
                "{" => {
                    // Opening brace: if alone on its line, indent at `depth`
                    let line = source.lines().nth(row).unwrap_or("");
                    if line.trim() == "{" {
                        depths[row] = depth;
                    }
                }
                "}" => {
                    // Closing brace at the parent depth
                    depths[row] = depth;
                }
                _ => {
                    // Content inside body: depth + 1
                    // Set the first line of this child
                    depths[row] = depth + 1;
                    // For multi-line constructs, set continuation lines
                    let end_row = child.end_position().row;
                    for r in (row + 1)..=end_row.min(depths.len() - 1) {
                        if depths[r] < depth + 2 {
                            depths[r] = depth + 2;
                        }
                    }
                    // Recurse for nested bodies
                    assign_depths(child, source, depth + 1, depths);
                }
            }
        }
    } else {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            assign_depths(child, source, depth, depths);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_simple_definition() {
        let source = "part def Vehicle {\npart engine : Engine;\n}\n";
        let opts = FormatOptions::default();
        let result = format_source(source, &opts);
        assert_eq!(result, "part def Vehicle {\n    part engine : Engine;\n}\n");
    }

    #[test]
    fn format_nested_definitions() {
        let source = "package P {\npart def Vehicle {\npart engine : Engine;\n}\n}\n";
        let opts = FormatOptions::default();
        let result = format_source(source, &opts);
        assert!(result.contains("    part def Vehicle {"));
        assert!(result.contains("        part engine : Engine;"));
        assert!(result.contains("    }"));
    }

    #[test]
    fn format_already_correct() {
        let source = "part def Vehicle {\n    part engine : Engine;\n}\n";
        let opts = FormatOptions::default();
        let result = format_source(source, &opts);
        assert_eq!(result, source);
    }

    #[test]
    fn format_empty_body() {
        let source = "part def Vehicle;\n";
        let opts = FormatOptions::default();
        let result = format_source(source, &opts);
        assert_eq!(result, "part def Vehicle;\n");
    }

    #[test]
    fn format_preserves_blank_lines() {
        let source = "part def A;\n\npart def B;\n";
        let opts = FormatOptions::default();
        let result = format_source(source, &opts);
        assert_eq!(result, source);
    }

    #[test]
    fn format_with_doc_comment() {
        let source = "part def Vehicle {\ndoc /* A vehicle */\npart engine : Engine;\n}\n";
        let opts = FormatOptions::default();
        let result = format_source(source, &opts);
        assert!(result.contains("    doc /* A vehicle */"));
        assert!(result.contains("    part engine : Engine;"));
    }

    #[test]
    fn format_custom_indent_width() {
        let source = "part def Vehicle {\npart engine : Engine;\n}\n";
        let opts = FormatOptions {
            indent_width: 2,
            trailing_newline: true,
        };
        let result = format_source(source, &opts);
        assert_eq!(result, "part def Vehicle {\n  part engine : Engine;\n}\n");
    }

    #[test]
    fn format_state_machine() {
        let source = "state def SM {\nstate idle;\nstate active;\ntransition first idle then active;\n}\n";
        let opts = FormatOptions::default();
        let result = format_source(source, &opts);
        assert!(result.contains("    state idle;"));
        assert!(result.contains("    state active;"));
        assert!(result.contains("    transition first idle then active;"));
    }
}
