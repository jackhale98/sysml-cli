use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenType, SemanticTokensLegend,
};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use sysml_core::parser::get_language;

/// The token types we emit, in legend order.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::COMMENT,      // 0
    SemanticTokenType::STRING,       // 1
    SemanticTokenType::NUMBER,       // 2
    SemanticTokenType::KEYWORD,      // 3
    SemanticTokenType::TYPE,         // 4
    SemanticTokenType::VARIABLE,     // 5
    SemanticTokenType::OPERATOR,     // 6
    SemanticTokenType::NAMESPACE,    // 7
    SemanticTokenType::DECORATOR,    // 8 - for @metadata
    SemanticTokenType::MODIFIER,     // 9 - visibility/abstract
];

pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.to_vec(),
        token_modifiers: vec![],
    }
}

/// Map a tree-sitter capture name to a token type index.
fn capture_to_token_type(capture_name: &str) -> Option<u32> {
    match capture_name {
        "comment" | "comment.documentation" => Some(0),
        "string" => Some(1),
        "number" => Some(2),
        "constant.builtin" => Some(2),
        "keyword" | "keyword.operator" => Some(3),
        "type.definition" | "type" => Some(4),
        "variable" => Some(5),
        "operator" => Some(6),
        "module" => Some(7),
        "attribute" => Some(8),
        "keyword.modifier" => Some(9),
        "punctuation.bracket" | "punctuation.delimiter" => None,
        _ => None,
    }
}

/// The embedded highlights query source (copied by build.rs).
const HIGHLIGHTS_SCM: &str = include_str!(concat!(env!("OUT_DIR"), "/highlights.scm"));

/// Compute semantic tokens for a source string.
pub fn semantic_tokens(source: &str) -> Vec<SemanticToken> {
    let language = get_language();

    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    let Ok(query) = Query::new(&language, HIGHLIGHTS_SCM) else {
        return Vec::new();
    };

    let mut cursor = QueryCursor::new();
    let source_bytes = source.as_bytes();
    let mut matches = cursor.matches(&query, tree.root_node(), source_bytes);

    // Collect raw tokens: (line, col, length, token_type)
    let mut raw_tokens: Vec<(u32, u32, u32, u32)> = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    // StreamingIterator: advance() then get()
    while let Some(m) = {
        matches.advance();
        matches.get()
    } {
        for capture in m.captures {
            let capture_name = &query.capture_names()[capture.index as usize];
            let Some(token_type) = capture_to_token_type(capture_name) else {
                continue;
            };

            let node = capture.node;
            let start = node.start_position();
            let end = node.end_position();

            if start.row == end.row {
                let length = (end.column - start.column) as u32;
                if length > 0 {
                    raw_tokens.push((start.row as u32, start.column as u32, length, token_type));
                }
            } else {
                // Multi-line token: emit per-line segments
                if let Some(line) = lines.get(start.row) {
                    let len = line.len().saturating_sub(start.column) as u32;
                    if len > 0 {
                        raw_tokens.push((
                            start.row as u32,
                            start.column as u32,
                            len,
                            token_type,
                        ));
                    }
                }
                for row in (start.row + 1)..end.row {
                    if let Some(line) = lines.get(row) {
                        let trimmed = line.trim_start();
                        let indent = line.len() - trimmed.len();
                        let len = trimmed.len() as u32;
                        if len > 0 {
                            raw_tokens.push((row as u32, indent as u32, len, token_type));
                        }
                    }
                }
                if end.column > 0 {
                    raw_tokens.push((end.row as u32, 0, end.column as u32, token_type));
                }
            }
        }
    }

    // Sort by (line, col)
    raw_tokens.sort_by_key(|t| (t.0, t.1));

    // Deduplicate at same position
    raw_tokens.dedup_by_key(|t| (t.0, t.1));

    // Delta-encode
    let mut result = Vec::with_capacity(raw_tokens.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for (line, col, length, token_type) in raw_tokens {
        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 {
            col - prev_start
        } else {
            col
        };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });

        prev_line = line;
        prev_start = col;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legend_has_expected_types() {
        let l = legend();
        assert!(l.token_types.contains(&SemanticTokenType::KEYWORD));
        assert!(l.token_types.contains(&SemanticTokenType::TYPE));
        assert!(l.token_types.contains(&SemanticTokenType::COMMENT));
        assert!(l.token_types.contains(&SemanticTokenType::STRING));
    }

    #[test]
    fn simple_definition_tokens() {
        let source = "part def Vehicle;\n";
        let tokens = semantic_tokens(source);
        assert!(
            !tokens.is_empty(),
            "should produce tokens for a definition"
        );
        let has_keyword = tokens.iter().any(|t| t.token_type == 3);
        let has_type = tokens.iter().any(|t| t.token_type == 4);
        assert!(has_keyword, "should have keyword tokens");
        assert!(has_type, "should have type tokens");
    }

    #[test]
    fn comment_tokens() {
        let source = "// this is a comment\npart def A;\n";
        let tokens = semantic_tokens(source);
        let has_comment = tokens.iter().any(|t| t.token_type == 0);
        assert!(has_comment, "should have comment token");
    }

    #[test]
    fn usage_variable_tokens() {
        let source = "part def Vehicle {\n    part engine : Engine;\n}\n";
        let tokens = semantic_tokens(source);
        let has_variable = tokens.iter().any(|t| t.token_type == 5);
        assert!(has_variable, "should have variable token for usage name");
    }

    #[test]
    fn delta_encoding_correct() {
        let source = "part def A;\npart def B;\n";
        let tokens = semantic_tokens(source);
        assert!(!tokens.is_empty());
        assert_eq!(tokens[0].delta_line, 0);
        let has_newline_delta = tokens.iter().any(|t| t.delta_line > 0);
        assert!(has_newline_delta, "should have delta_line > 0 for second line tokens");
    }

    #[test]
    fn empty_source_no_tokens() {
        let tokens = semantic_tokens("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn operator_tokens() {
        let source = "part def A :> B;\n";
        let tokens = semantic_tokens(source);
        assert!(!tokens.is_empty());
    }

    #[test]
    fn keyword_modifier_tokens() {
        let source = "abstract part def Vehicle;\n";
        let tokens = semantic_tokens(source);
        let has_modifier = tokens.iter().any(|t| t.token_type == 9);
        assert!(has_modifier, "should have modifier token for 'abstract'");
    }

    #[test]
    fn package_namespace_token() {
        let source = "package Vehicles {\n}\n";
        let tokens = semantic_tokens(source);
        let has_namespace = tokens.iter().any(|t| t.token_type == 7);
        assert!(has_namespace, "should have namespace token for package name");
    }
}
