//! Turns `pblang::parse` errors into LSP `Diagnostic`s.

use std::ops::Range;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::pblang::{self, ParseError};

use super::document::OpenDocument;

/// Extracts the byte span and a human-readable message from any
/// `ParseError` variant lalrpop can produce for pblang.
fn error_span_and_message(source: &str, err: &ParseError) -> (Range<usize>, String) {
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

impl OpenDocument {
    /// Parses `self.text` and returns the diagnostics for it — empty if it
    /// parses successfully with no recovered errors, which is what clears
    /// any previously published diagnostics. Reparses rather than using the
    /// cached tree, since it needs the *errors* a parse produces, which the
    /// cached tree (just `None` on total failure) doesn't carry. One
    /// diagnostic per recovered error (a malformed `let`/`bind` binding or
    /// object assignment — see `grammar.lalrpop`), plus one more if parsing
    /// still failed outright somewhere recovery couldn't help.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let parsed = pblang::parse(&self.text);
        let mut diagnostics: Vec<Diagnostic> = parsed
            .errors
            .iter()
            .map(|recovery| self.diagnostic_for(&recovery.error))
            .collect();
        if let Err(fatal) = &parsed.tree {
            diagnostics.push(self.diagnostic_for(fatal));
        }
        diagnostics
    }

    fn diagnostic_for(&self, err: &ParseError) -> Diagnostic {
        let (span, message) = error_span_and_message(&self.text, err);
        Diagnostic {
            range: self.line_index.range(&self.text, span),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("pblang".to_string()),
            message,
            ..Diagnostic::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_source_has_no_diagnostics() {
        let doc = OpenDocument::new("null".to_string());
        assert_eq!(doc.diagnostics(), Vec::new());
    }

    #[test]
    fn invalid_source_has_one_diagnostic_with_a_message() {
        let doc = OpenDocument::new("let x = in x".to_string());
        let diagnostics = doc.diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert!(!diagnostics[0].message.is_empty());
    }

    #[test]
    fn two_malformed_bindings_produce_two_diagnostics() {
        let doc = OpenDocument::new("bind x = ; y = ; in null".to_string());
        let diagnostics = doc.diagnostics();
        assert_eq!(diagnostics.len(), 2);
        assert!(!diagnostics[0].message.is_empty());
        assert!(!diagnostics[1].message.is_empty());
        assert_ne!(diagnostics[0].range, diagnostics[1].range);
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
            let doc = OpenDocument::new(source);
            assert_eq!(doc.diagnostics(), Vec::new(), "fixture {}", path.display());
        }
    }
}
