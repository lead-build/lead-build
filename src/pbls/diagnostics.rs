//! Turns `pblang::parse` errors into LSP `Diagnostic`s.

use std::ops::Range;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::pblang::{self, ParseError};

use super::convert::LineIndex;

/// Extracts the byte span and a human-readable message from any
/// `ParseError` variant lalrpop can produce for pblang.
fn error_span_and_message(source: &str, err: &ParseError<'_>) -> (Range<usize>, String) {
    let (span, message) = match err {
        ParseError::InvalidToken { location } => {
            (*location..*location, "invalid token".to_string())
        }
        ParseError::UnrecognizedEof { location, expected } => (
            *location..*location,
            format!(
                "unexpected end of file, expected one of: {}",
                expected.join(", ")
            ),
        ),
        ParseError::UnrecognizedToken {
            token: (start, tok, end),
            expected,
        } => (
            *start..*end,
            format!(
                "unexpected token {tok}, expected one of: {}",
                expected.join(", ")
            ),
        ),
        ParseError::ExtraToken {
            token: (start, tok, end),
        } => (*start..*end, format!("unexpected extra token {tok}")),
        ParseError::User { error } => (error.span.clone(), error.message.clone()),
    };
    // Clamp to the source length: lalrpop can report a location one past the
    // last byte (e.g. at EOF), which is still a valid empty range.
    let len = source.len();
    (span.start.min(len)..span.end.min(len), message)
}

/// Parses `source` and returns the diagnostics for it — empty if it parses
/// successfully, which is what clears any previously published diagnostics.
/// lalrpop stops at the first error, so this is never more than one
/// diagnostic today.
pub fn diagnostics_for_source(source: &str, line_index: &LineIndex) -> Vec<Diagnostic> {
    match pblang::parse(source) {
        Ok(_) => Vec::new(),
        Err(err) => {
            let (span, message) = error_span_and_message(source, &err);
            vec![Diagnostic {
                range: line_index.range(source, span),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("pblang".to_string()),
                message,
                ..Diagnostic::default()
            }]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_source_has_no_diagnostics() {
        let source = "null";
        let line_index = LineIndex::new(source);
        assert_eq!(diagnostics_for_source(source, &line_index), Vec::new());
    }

    #[test]
    fn invalid_source_has_one_diagnostic_with_a_message() {
        let source = "let x = in x";
        let line_index = LineIndex::new(source);
        let diagnostics = diagnostics_for_source(source, &line_index);
        assert_eq!(diagnostics.len(), 1);
        assert!(!diagnostics[0].message.is_empty());
    }

    #[test]
    fn all_valid_fixtures_have_no_diagnostics() {
        use std::fs;
        use std::path::{Path, PathBuf};

        fn fixture_files(root: &Path) -> Vec<PathBuf> {
            let mut files = Vec::new();
            for entry in fs::read_dir(root).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    files.extend(fixture_files(&path));
                } else if path.file_name().is_some_and(|name| name == "main.pbb") {
                    files.push(path);
                }
            }
            files
        }

        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        for path in fixture_files(&fixture_root) {
            let source = fs::read_to_string(&path).unwrap();
            let line_index = LineIndex::new(&source);
            assert_eq!(
                diagnostics_for_source(&source, &line_index),
                Vec::new(),
                "fixture {}",
                path.display()
            );
        }
    }
}
