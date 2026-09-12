pub mod fmt;
pub mod lexer;
pub mod syntaxtree;
pub mod visit;

use lalrpop_util::lalrpop_mod;
use lexer::{LexError, Lexer};
use syntaxtree::SyntaxNode;

lalrpop_mod!(
    #[allow(clippy::ptr_arg)]
    #[allow(clippy::type_complexity)]
    pub grammar,
    "pblang/grammar.rs"
);

pub type ParseError = lalrpop_util::ParseError<usize, lexer::Tok, LexError>;
pub type ErrorRecovery = lalrpop_util::ErrorRecovery<usize, lexer::Tok, LexError>;

/// The result of parsing a `.pbb` source string: `tree` is `Err` only when
/// parsing failed somewhere LALRPOP's error recovery (see
/// `grammar.lalrpop`'s `LetSetStmt`/`BindSetStmt`/`AssignStmt`) couldn't
/// help — everywhere else, a single bad binding/assignment becomes a
/// `SyntaxKind::ERROR` token in an otherwise-valid tree, and is recorded in
/// `errors` instead of aborting the parse. `errors` can be non-empty
/// alongside either outcome, since it's populated incrementally as parsing
/// proceeds, independent of whether a later, unrecoverable error still
/// aborts the overall parse.
pub struct Parsed {
    pub tree: std::result::Result<SyntaxNode, ParseError>,
    pub errors: Vec<ErrorRecovery>,
}

pub fn parse(code: &str) -> Parsed {
    let tokens = Lexer::new(code);
    let mut errors = Vec::new();
    let tree = grammar::ExprParser::new()
        .parse(code, &mut errors, tokens)
        .map(|root| {
            SyntaxNode::new_root(
                root.green
                    .into_node()
                    .expect("the top-level Expr rule always produces a node"),
            )
        });
    Parsed { tree, errors }
}

#[cfg(test)]
mod tests {
    use super::parse;
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
        files.sort();
        files
    }

    #[test]
    fn fixture_files_round_trip_through_cst() {
        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

        for path in fixture_files(&fixture_root) {
            let source = fs::read_to_string(&path).unwrap();
            let parsed = parse(&source);
            let tree = parsed
                .tree
                .unwrap_or_else(|error| panic!("failed to parse {}: {error:?}", path.display()));
            assert!(
                parsed.errors.is_empty(),
                "fixture {} produced recovered parse errors: {:?}",
                path.display(),
                parsed.errors
            );
            let output = tree.text().to_string();

            // The tree is byte-exact within the parsed expression's own
            // span, comments included (TRIVIA holds the real source bytes
            // for gaps between tokens). Trivia outside that span — e.g. a
            // trailing comment after the file's last token — isn't
            // captured, so this still strips whitespace before comparing
            // rather than requiring byte-for-byte equality.
            let clean_source: String = source.chars().filter(|c| !c.is_whitespace()).collect();
            let clean_output: String = output.chars().filter(|c| !c.is_whitespace()).collect();

            assert_eq!(clean_output, clean_source, "fixture {}", path.display());
        }
    }
}
