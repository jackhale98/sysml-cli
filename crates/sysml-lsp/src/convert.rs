use sysml_core::model::Span;
use tower_lsp_server::ls_types::{Position, Range};

/// Convert a sysml-core Span to an LSP Range with UTF-16 columns (the LSP
/// default position encoding). Uses the span's byte offsets against
/// `source`; falls back to the span's raw (byte) columns when the offsets
/// don't fit the given source (e.g. unavailable source passed as "").
pub fn span_to_range(span: &Span, source: &str) -> Range {
    if span.end_byte <= source.len() && span.start_byte <= span.end_byte {
        return Range::new(
            offset_to_position(source, span.start_byte),
            offset_to_position(source, span.end_byte),
        );
    }
    Range::new(
        Position::new(
            span.start_row.saturating_sub(1) as u32,
            span.start_col.saturating_sub(1) as u32,
        ),
        Position::new(
            span.end_row.saturating_sub(1) as u32,
            span.end_col.saturating_sub(1) as u32,
        ),
    )
}

/// Convert an LSP Position (0-based line, UTF-16 character) to a byte
/// offset in source text. Returns None if the position is out of range.
pub fn position_to_offset(source: &str, pos: &Position) -> Option<usize> {
    let mut offset = 0usize;
    for (i, line) in source.split('\n').enumerate() {
        if i == pos.line as usize {
            let col = pos.character as usize;
            // Walk characters accumulating UTF-16 code units
            let mut units = 0usize;
            for (byte_idx, ch) in line.char_indices() {
                if units == col {
                    return Some(offset + byte_idx);
                }
                units += ch.len_utf16();
            }
            // Column is at or past end of line
            if units == col {
                return Some(offset + line.len());
            }
            return None;
        }
        offset += line.len() + 1; // +1 for the '\n'
    }
    None
}

/// Convert a byte offset in source text to an LSP Position (0-based line,
/// UTF-16 character).
pub fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (i, ch) in source.char_indices() {
        if i == offset {
            let col = source[line_start..offset].encode_utf16().count() as u32;
            return Position::new(line, col);
        }
        if ch == '\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    // Offset is at EOF
    let col = source[line_start..].encode_utf16().count() as u32;
    Position::new(line, col)
}

/// UTF-16 code-unit column for a byte column within a single line.
/// Clamps to the line's end when the byte column overshoots.
pub fn utf16_col(line: &str, byte_col: usize) -> u32 {
    let upto = line
        .char_indices()
        .take_while(|(i, _)| *i < byte_col.min(line.len()))
        .map(|(_, c)| c.len_utf16())
        .sum::<usize>();
    upto as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_to_range_first_line() {
        let span = Span {
            start_row: 1,
            start_col: 1,
            end_row: 1,
            end_col: 10,
            start_byte: 0,
            end_byte: 9,
        };
        let range = span_to_range(&span, "");
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 9);
    }

    #[test]
    fn span_to_range_middle_line() {
        let span = Span {
            start_row: 5,
            start_col: 3,
            end_row: 5,
            end_col: 15,
            start_byte: 40,
            end_byte: 52,
        };
        let range = span_to_range(&span, "");
        assert_eq!(range.start.line, 4);
        assert_eq!(range.start.character, 2);
        assert_eq!(range.end.line, 4);
        assert_eq!(range.end.character, 14);
    }

    #[test]
    fn span_to_range_multi_line() {
        let span = Span {
            start_row: 2,
            start_col: 1,
            end_row: 4,
            end_col: 2,
            start_byte: 10,
            end_byte: 30,
        };
        let range = span_to_range(&span, "");
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 3);
        assert_eq!(range.end.character, 1);
    }

    #[test]
    fn position_to_offset_single_line() {
        let source = "hello world";
        let pos = Position::new(0, 6);
        assert_eq!(position_to_offset(source, &pos), Some(6));
    }

    #[test]
    fn position_to_offset_multi_line() {
        let source = "line one\nline two\nline three";
        // "line two" starts at byte 9, col 5 = byte 14
        let pos = Position::new(1, 5);
        assert_eq!(position_to_offset(source, &pos), Some(14));
    }

    #[test]
    fn position_to_offset_utf8_multibyte() {
        let source = "café latte";
        // 'é' is 2 bytes, so character index 4 (space) is at byte 5
        let pos = Position::new(0, 5);
        assert_eq!(position_to_offset(source, &pos), Some(6));
    }

    #[test]
    fn position_to_offset_at_eof() {
        let source = "abc";
        let pos = Position::new(0, 3);
        assert_eq!(position_to_offset(source, &pos), Some(3));
    }

    #[test]
    fn position_to_offset_empty_file() {
        let source = "";
        let pos = Position::new(0, 0);
        assert_eq!(position_to_offset(source, &pos), Some(0));
    }

    #[test]
    fn position_to_offset_out_of_range() {
        let source = "abc";
        let pos = Position::new(1, 0);
        assert_eq!(position_to_offset(source, &pos), None);
    }

    #[test]
    fn offset_to_position_start() {
        let source = "hello\nworld";
        let pos = offset_to_position(source, 0);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn offset_to_position_second_line() {
        let source = "hello\nworld";
        let pos = offset_to_position(source, 8);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 2);
    }

    #[test]
    fn offset_to_position_eof() {
        let source = "abc\ndef";
        let pos = offset_to_position(source, 7);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 3);
    }

    #[test]
    fn offset_to_position_utf8() {
        let source = "café";
        // byte offset 5 = after 'é', which is character index 4
        let pos = offset_to_position(source, 5);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 4);
    }

    #[test]
    fn roundtrip_position_offset() {
        let source = "first line\nsecond line\nthird line";
        let original = Position::new(2, 5);
        let offset = position_to_offset(source, &original).unwrap();
        let back = offset_to_position(source, offset);
        assert_eq!(back, original);
    }
}
