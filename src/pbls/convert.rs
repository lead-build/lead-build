//! Conversion between pblang's byte-offset spans (as produced by
//! [`crate::pblang::parse`] and its error types) and LSP's UTF-16-based
//! `Position`/`Range`.

use std::ops::Range;

use tower_lsp::lsp_types::{Position, Range as LspRange};

/// Byte offsets of each line's start within a document, so a byte offset can
/// be converted to an LSP `Position` (UTF-16 code-unit `character` within a
/// `line`) without rescanning the whole document from the start each time.
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    /// Scans `text` for line breaks and records where each line starts.
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    /// Converts a byte offset into `text` to an LSP `Position`. `text` must
    /// be the same source this index was built from.
    pub fn position(&self, text: &str, offset: usize) -> Position {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line - 1,
        };
        let line_start = self.line_starts[line];
        let character = text[line_start..offset].encode_utf16().count() as u32;
        Position {
            line: line as u32,
            character,
        }
    }

    /// Converts a byte range into `text` to an LSP `Range`.
    pub fn range(&self, text: &str, span: Range<usize>) -> LspRange {
        LspRange {
            start: self.position(text, span.start),
            end: self.position(text, span.end),
        }
    }

    /// Converts an LSP `Position` back to a byte offset into `text` — the
    /// inverse of [`Self::position`]. `text` must be the same source this
    /// index was built from. Clamps out-of-range input (a line beyond the
    /// document, or a character count past the line's actual UTF-16
    /// length) to the nearest valid offset, rather than panicking, since a
    /// client's position can be stale by the time a request is handled.
    pub fn offset(&self, text: &str, position: Position) -> usize {
        let line = (position.line as usize).min(self.line_starts.len() - 1);
        let line_start = self.line_starts[line];
        let line_end = self
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(text.len());
        let line_text = &text[line_start..line_end];
        let mut utf16_used = 0u32;
        for (byte_idx, ch) in line_text.char_indices() {
            if utf16_used >= position.character {
                return line_start + byte_idx;
            }
            utf16_used += ch.len_utf16() as u32;
        }
        line_start + line_text.trim_end_matches(['\n', '\r']).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_within_single_line() {
        let text = "hello world";
        let index = LineIndex::new(text);
        assert_eq!(
            index.position(text, 6),
            Position {
                line: 0,
                character: 6
            }
        );
    }

    #[test]
    fn position_across_multiple_lines() {
        let text = "foo\nbar\nbaz";
        let index = LineIndex::new(text);
        assert_eq!(
            index.position(text, 0),
            Position {
                line: 0,
                character: 0
            }
        );
        // 'b' of "bar", right after the first '\n'.
        assert_eq!(
            index.position(text, 4),
            Position {
                line: 1,
                character: 0
            }
        );
        // 'z' of "baz".
        assert_eq!(
            index.position(text, 10),
            Position {
                line: 2,
                character: 2
            }
        );
    }

    #[test]
    fn position_at_line_boundary_offset() {
        let text = "ab\ncd";
        let index = LineIndex::new(text);
        // Offset right at the '\n' itself is still on line 0.
        assert_eq!(
            index.position(text, 2),
            Position {
                line: 0,
                character: 2
            }
        );
        // Offset right after the '\n' is the start of line 1.
        assert_eq!(
            index.position(text, 3),
            Position {
                line: 1,
                character: 0
            }
        );
    }

    #[test]
    fn position_counts_utf16_code_units_not_bytes() {
        // "é" is 2 bytes in UTF-8 but 1 UTF-16 code unit.
        // "😀" is 4 bytes in UTF-8 but 2 UTF-16 code units (a surrogate pair).
        let text = "é😀x";
        let index = LineIndex::new(text);
        let x_byte_offset = "é😀".len();
        assert_eq!(
            index.position(text, x_byte_offset),
            Position {
                line: 0,
                character: 3 // 1 (é) + 2 (😀 surrogate pair)
            }
        );
    }

    #[test]
    fn range_converts_both_ends() {
        let text = "foo\nbar";
        let index = LineIndex::new(text);
        assert_eq!(
            index.range(text, 1..5),
            LspRange {
                start: Position {
                    line: 0,
                    character: 1
                },
                end: Position {
                    line: 1,
                    character: 1
                },
            }
        );
    }

    #[test]
    fn offset_within_single_line() {
        let text = "hello world";
        let index = LineIndex::new(text);
        assert_eq!(
            index.offset(
                text,
                Position {
                    line: 0,
                    character: 6
                }
            ),
            6
        );
    }

    #[test]
    fn offset_across_multiple_lines() {
        let text = "foo\nbar\nbaz";
        let index = LineIndex::new(text);
        assert_eq!(
            index.offset(
                text,
                Position {
                    line: 1,
                    character: 0
                }
            ),
            4
        );
        assert_eq!(
            index.offset(
                text,
                Position {
                    line: 2,
                    character: 2
                }
            ),
            10
        );
    }

    #[test]
    fn offset_counts_utf16_code_units_not_bytes() {
        // Same fixture as `position_counts_utf16_code_units_not_bytes`,
        // inverted: character 3 (1 for "é" + 2 for the "😀" surrogate
        // pair) lands right at "x"'s byte offset.
        let text = "é😀x";
        let index = LineIndex::new(text);
        let x_byte_offset = "é😀".len();
        assert_eq!(
            index.offset(
                text,
                Position {
                    line: 0,
                    character: 3
                }
            ),
            x_byte_offset
        );
    }

    #[test]
    fn offset_clamps_character_past_end_of_line() {
        let text = "ab\ncd";
        let index = LineIndex::new(text);
        // Line 0 is "ab" (2 UTF-16 units); asking for character 10 clamps
        // to the end of that line's content, not into the line break.
        assert_eq!(
            index.offset(
                text,
                Position {
                    line: 0,
                    character: 10
                }
            ),
            2
        );
    }

    #[test]
    fn offset_clamps_line_past_end_of_document() {
        let text = "ab\ncd";
        let index = LineIndex::new(text);
        assert_eq!(
            index.offset(
                text,
                Position {
                    line: 99,
                    character: 0
                }
            ),
            3
        );
    }

    #[test]
    fn position_and_offset_round_trip() {
        let text = "let x = 1;\nin x + é😀";
        let index = LineIndex::new(text);
        for byte_offset in 0..=text.len() {
            if !text.is_char_boundary(byte_offset) {
                continue;
            }
            let position = index.position(text, byte_offset);
            assert_eq!(index.offset(text, position), byte_offset);
        }
    }
}
