//! Byte offset ↔ line/column mapping for a source text.
//!
//! [`LineIndex`] precomputes a newline table (rust-analyzer style) so that
//! repeated conversions between byte offsets and line/column positions are
//! cheap. Columns are available both as UTF-8 byte columns (the convention
//! used by [`crate::span::Span`]) and as UTF-16 code-unit columns (the
//! convention used by LSP).
//!
//! Lines and columns are 1-based throughout, matching `Span`.

/// A 1-based line/column position. The unit of `col` depends on which
/// [`LineIndex`] accessor produced (or consumes) it: UTF-8 bytes for
/// [`LineIndex::line_col`]/[`LineIndex::offset`], UTF-16 code units for
/// [`LineIndex::line_col_utf16`]/[`LineIndex::offset_utf16`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// Precomputed newline table for a source text.
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    text: &'a str,
    /// Byte offset of the start of each line. `line_starts[0] == 0`.
    line_starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            text.bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self { text, line_starts }
    }

    /// Number of lines in the text (a trailing newline starts a final empty line).
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// The 0-based index into `line_starts` for the line containing `offset`.
    fn line_idx(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&start| start <= offset) - 1
    }

    /// Exclusive end offset of the 0-based line `idx`: the position of its
    /// terminating `\n`, or `text.len()` for the last line.
    fn line_end(&self, idx: usize) -> usize {
        match self.line_starts.get(idx + 1) {
            Some(&next_start) => next_start - 1,
            None => self.text.len(),
        }
    }

    /// Map a byte offset to a (line, UTF-8 byte column) position.
    /// Returns `None` if `offset > text.len()`. `offset == text.len()` maps
    /// to one past the end of the last line.
    pub fn line_col(&self, offset: usize) -> Option<LineCol> {
        if offset > self.text.len() {
            return None;
        }
        let idx = self.line_idx(offset);
        Some(LineCol {
            line: (idx + 1) as u32,
            col: (offset - self.line_starts[idx] + 1) as u32,
        })
    }

    /// Map a (line, UTF-8 byte column) position back to a byte offset.
    /// Returns `None` if the line does not exist or the column runs past the
    /// end of the line (column `len + 1`, pointing at the newline or EOF, is
    /// allowed).
    pub fn offset(&self, pos: LineCol) -> Option<usize> {
        if pos.line == 0 || pos.col == 0 {
            return None;
        }
        let idx = (pos.line - 1) as usize;
        let start = *self.line_starts.get(idx)?;
        let offset = start + (pos.col - 1) as usize;
        (offset <= self.line_end(idx)).then_some(offset)
    }

    /// Map a byte offset to a (line, UTF-16 code-unit column) position.
    /// Returns `None` if `offset` is out of range or not on a `char` boundary.
    pub fn line_col_utf16(&self, offset: usize) -> Option<LineCol> {
        if offset > self.text.len() {
            return None;
        }
        let idx = self.line_idx(offset);
        let prefix = self.text.get(self.line_starts[idx]..offset)?;
        let col16: usize = prefix.chars().map(char::len_utf16).sum();
        Some(LineCol {
            line: (idx + 1) as u32,
            col: (col16 + 1) as u32,
        })
    }

    /// Map a (line, UTF-16 code-unit column) position back to a byte offset.
    /// Returns `None` if the line does not exist, the column runs past the
    /// end of the line, or the column lands inside a surrogate pair.
    pub fn offset_utf16(&self, pos: LineCol) -> Option<usize> {
        if pos.line == 0 || pos.col == 0 {
            return None;
        }
        let idx = (pos.line - 1) as usize;
        let start = *self.line_starts.get(idx)?;
        let line_text = &self.text[start..self.line_end(idx)];

        let mut remaining = (pos.col - 1) as usize;
        let mut offset = start;
        for ch in line_text.chars() {
            if remaining == 0 {
                break;
            }
            let units = ch.len_utf16();
            if remaining < units {
                return None; // inside a surrogate pair
            }
            remaining -= units;
            offset += ch.len_utf8();
        }
        (remaining == 0).then_some(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lc(line: u32, col: u32) -> LineCol {
        LineCol { line, col }
    }

    #[test]
    fn test_ascii_round_trip() {
        let text = "abc\ndef\n\nlast";
        let index = LineIndex::new(text);

        assert_eq!(index.line_count(), 4);
        assert_eq!(index.line_col(0), Some(lc(1, 1)));
        assert_eq!(index.line_col(3), Some(lc(1, 4))); // at the '\n'
        assert_eq!(index.line_col(4), Some(lc(2, 1)));
        assert_eq!(index.line_col(8), Some(lc(3, 1))); // empty line
        assert_eq!(index.line_col(9), Some(lc(4, 1)));
        assert_eq!(index.line_col(text.len()), Some(lc(4, 5))); // EOF
        assert_eq!(index.line_col(text.len() + 1), None);

        assert_eq!(index.offset(lc(1, 1)), Some(0));
        assert_eq!(index.offset(lc(2, 2)), Some(5));
        assert_eq!(index.offset(lc(4, 5)), Some(text.len()));
        assert_eq!(index.offset(lc(2, 9)), None); // past end of line
        assert_eq!(index.offset(lc(5, 1)), None); // no such line
        assert_eq!(index.offset(lc(0, 1)), None);
    }

    #[test]
    fn test_multibyte_utf8_columns() {
        // 'é' is 2 bytes in UTF-8, 1 UTF-16 unit.
        let text = "café x\nsecond";
        let index = LineIndex::new(text);

        // Offset of 'x': "caf" (3) + "é" (2) + " " (1) = 6.
        assert_eq!(index.line_col(6), Some(lc(1, 7)));
        assert_eq!(index.line_col_utf16(6), Some(lc(1, 6)));

        // Both directions.
        assert_eq!(index.offset(lc(1, 7)), Some(6));
        assert_eq!(index.offset_utf16(lc(1, 6)), Some(6));

        // Offset 4 is inside 'é': byte column still reported, UTF-16 is None.
        assert_eq!(index.line_col(4), Some(lc(1, 5)));
        assert_eq!(index.line_col_utf16(4), None);
    }

    #[test]
    fn test_emoji_utf16_columns() {
        // '🦀' is 4 bytes in UTF-8 and 2 UTF-16 code units (a surrogate pair).
        let text = "ab🦀cd\nx🦀";
        let index = LineIndex::new(text);

        // Offset of 'c' on line 1: 2 + 4 = 6.
        assert_eq!(index.line_col(6), Some(lc(1, 7)));
        assert_eq!(index.line_col_utf16(6), Some(lc(1, 5)));
        assert_eq!(index.offset(lc(1, 7)), Some(6));
        assert_eq!(index.offset_utf16(lc(1, 5)), Some(6));

        // A UTF-16 column inside the surrogate pair maps to no offset.
        assert_eq!(index.offset_utf16(lc(1, 4)), None);

        // End of line 2: "x" (1) + "🦀" (4) = offset 12 = len.
        let eof = text.len();
        assert_eq!(index.line_col(eof), Some(lc(2, 6)));
        assert_eq!(index.line_col_utf16(eof), Some(lc(2, 4)));
        assert_eq!(index.offset_utf16(lc(2, 4)), Some(eof));
        assert_eq!(index.offset_utf16(lc(2, 5)), None); // past end of line
    }

    #[test]
    fn test_round_trip_every_char_boundary() {
        let text = "héllo 🦀\nwörld é🦀\n";
        let index = LineIndex::new(text);
        for (offset, _) in text.char_indices() {
            let lc8 = index.line_col(offset).unwrap();
            assert_eq!(index.offset(lc8), Some(offset), "utf8 at {offset}");
            let lc16 = index.line_col_utf16(offset).unwrap();
            assert_eq!(index.offset_utf16(lc16), Some(offset), "utf16 at {offset}");
        }
    }

    #[test]
    fn test_spans_agree_with_line_index() {
        // The parser's inline spans and LineIndex must agree on line/col.
        let src = "# head\n\ncafé **gras**\n";
        let index = LineIndex::new(src);
        let result = crate::parse_document(src);
        for block in &result.document.children {
            if let crate::ast::Block::Paragraph(p) = block {
                for seg in &p.content.segments {
                    let pos = index.line_col(seg.span.start).unwrap();
                    assert_eq!(pos.line, seg.span.line);
                    assert_eq!(pos.col, seg.span.col);
                }
            }
        }
    }
}
