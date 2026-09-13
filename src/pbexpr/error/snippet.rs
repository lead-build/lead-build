//! Clang-like source snippet rendering: given a source string and a byte
//! span within it, print a line-numbered excerpt of the surrounding lines,
//! with the span highlighted inline (in color, on a TTY) or left as plain
//! text (no color available — e.g. output is redirected). Deliberately
//! independent of `Loc`/`F` so it's directly testable with plain strings.

use std::fmt;

use super::Span;

/// Lines of context kept on each side of the span's start/end line.
const CONTEXT_LINES: usize = 1;

/// Below this many lines of gap between the "start" and "end" context
/// windows, just show everything contiguously instead of eliding with `...`.
const MAX_MERGED_GAP: usize = 2;

/// Bold red, matching the conventional color for the erroring span.
const COLOR_START: &str = "\x1b[1;31m";
const COLOR_END: &str = "\x1b[0m";

/// Byte offset where each line starts (`line_starts[0] == 0`). Line `i`
/// spans `line_starts[i]..line_starts.get(i + 1).unwrap_or(source.len())`.
fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Splits `source` into lines (trailing `\n`/`\r` stripped) using
/// `line_starts`, so line indices always line up 1:1 with `line_starts` —
/// unlike `str::lines()`, which drops a trailing empty line after a final
/// `\n`, this never mismatches.
fn split_lines<'a>(source: &'a str, line_starts: &[usize]) -> Vec<&'a str> {
    line_starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let end = line_starts.get(i + 1).copied().unwrap_or(source.len());
            let raw = &source[start..end];
            let raw = raw.strip_suffix('\n').unwrap_or(raw);
            raw.strip_suffix('\r').unwrap_or(raw)
        })
        .collect()
}

/// Maps a byte offset to a 0-indexed `(line, column)` pair.
fn line_and_col(line_starts: &[usize], offset: usize) -> (usize, usize) {
    let line = match line_starts.binary_search(&offset) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let col = offset - line_starts[line];
    (line, col)
}

/// Wraps the byte range `start..=end` of `line` in color escapes. Falls
/// back to the plain line if the range is empty or out of bounds.
fn highlight(line: &str, start: usize, end: usize) -> String {
    let end = end.min(line.len().saturating_sub(1));
    if line.is_empty() || start > end {
        return line.to_string();
    }
    format!(
        "{}{COLOR_START}{}{COLOR_END}{}",
        &line[..start],
        &line[start..=end],
        &line[end + 1..],
    )
}

/// Renders a snippet of `source` around `span` to `f`, each line prefixed
/// with `indent`. When `color` is true, the span is highlighted inline on
/// every line it touches; when false, lines are printed as plain text with
/// no marker at all — a no-color terminal (or a redirected/piped output)
/// gets the surrounding source only, not an ASCII caret approximation.
/// Byte offsets are used directly as columns (matching the existing
/// `path:line:col` convention elsewhere), so highlighting can be slightly
/// off for non-ASCII source text — an accepted simplification.
pub(super) fn format_snippet(
    f: &mut fmt::Formatter<'_>,
    source: &str,
    span: &Span,
    indent: &str,
    color: bool,
) -> fmt::Result {
    let starts = line_starts(source);
    let lines = split_lines(source, &starts);
    let total_lines = lines.len();

    let span_start = span.start.min(source.len());
    let span_end = span.end.min(source.len());
    let last_byte = if span_end > span_start {
        span_end - 1
    } else {
        span_start
    };

    let (start_line, start_col) = line_and_col(&starts, span_start);
    let (end_line, end_col) = line_and_col(&starts, last_byte);

    let begin_first = start_line.saturating_sub(CONTEXT_LINES);
    let begin_last = (start_line + CONTEXT_LINES).min(total_lines - 1);
    let end_first = end_line.saturating_sub(CONTEXT_LINES);
    let end_last = (end_line + CONTEXT_LINES).min(total_lines - 1);

    let gap = end_first as isize - begin_last as isize - 1;

    let width = (end_last + 1).to_string().len();

    let print_range = |f: &mut fmt::Formatter<'_>, from: usize, to: usize| -> fmt::Result {
        for (idx, &line) in lines.iter().enumerate().take(to + 1).skip(from) {
            if color && idx >= start_line && idx <= end_line {
                let mark_start = if idx == start_line { start_col } else { 0 };
                let mark_end = if idx == end_line {
                    end_col
                } else {
                    line.len().saturating_sub(1)
                };
                let highlighted = highlight(line, mark_start, mark_end);
                writeln!(f, "{indent}{:width$} | {}", idx + 1, highlighted, width = width)?;
            } else {
                writeln!(f, "{indent}{:width$} | {}", idx + 1, line, width = width)?;
            }
        }
        Ok(())
    };

    if gap <= MAX_MERGED_GAP as isize {
        print_range(f, begin_first, end_last)?;
    } else {
        print_range(f, begin_first, begin_last)?;
        writeln!(f, "{indent}...")?;
        print_range(f, end_first, end_last)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(source: &str, span: Span, color: bool) -> String {
        struct Wrap<'a>(&'a str, Span, bool);
        impl fmt::Display for Wrap<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                format_snippet(f, self.0, &self.1, "", self.2)
            }
        }
        Wrap(source, span, color).to_string()
    }

    #[test]
    fn no_color_prints_plain_lines_with_no_marker() {
        let source = "let a = 1;\nlet b = a + a;\nc\n";
        let start = source.find("a + a").unwrap();
        let out = render(source, start..start + 1, false);
        assert_eq!(out, "1 | let a = 1;\n2 | let b = a + a;\n3 | c\n");
        assert!(!out.contains('^'));
        assert!(!out.contains('\x1b'));
    }

    #[test]
    fn color_highlights_the_span_inline() {
        let source = "let a = 1;\nlet b = a + a;\nc\n";
        let start = source.find("a + a").unwrap();
        let out = render(source, start..start + 1, true);
        assert!(out.contains(COLOR_START));
        assert!(out.contains(COLOR_END));
        assert!(out.contains("let b = \x1b[1;31ma\x1b[0m + a;"));
        assert!(!out.contains('^'));
    }

    #[test]
    fn span_at_start_of_file_has_no_line_before() {
        let source = "bad\nnext\n";
        let out = render(source, 0..3, false);
        assert_eq!(out, "1 | bad\n2 | next\n");
    }

    #[test]
    fn span_at_end_of_file_has_no_line_after() {
        let source = "prev\nbad";
        let start = source.find("bad").unwrap();
        let out = render(source, start..source.len(), false);
        assert_eq!(out, "1 | prev\n2 | bad\n");
    }

    #[test]
    fn small_gap_multiline_span_is_shown_contiguously() {
        let source = "a\nb\nc\nd\ne\n";
        // span from "b" (line 2) to "d" (line 4): windows are lines 1-3 and 3-5,
        // touching with zero gap, so everything 1..=5 should print with no `...`.
        let b = source.find('b').unwrap();
        let d = source.find('d').unwrap();
        let out = render(source, b..d + 1, false);
        assert!(!out.contains("..."));
        assert!(out.contains("1 | a"));
        assert!(out.contains("5 | e"));
    }

    #[test]
    fn large_gap_multiline_span_is_elided() {
        let source = (0..20)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = source.find("line1\n").unwrap();
        let end = source.find("line18").unwrap();
        let out = render(&source, start..end + 1, false);
        assert!(out.contains("..."));
        assert!(out.contains("line0"));
        assert!(out.contains("line19"));
        assert!(!out.contains("line9\n"));
    }
}
