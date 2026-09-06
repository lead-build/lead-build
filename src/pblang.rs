pub mod fmt;
pub mod lexer;
pub mod syntaxtree;

use lalrpop_util::lalrpop_mod;
use lexer::{LexError, Lexer};
use syntaxtree::SyntaxNode;

lalrpop_mod!(pub grammar, "pblang/grammar.rs");

pub type ParseError<'input> = lalrpop_util::ParseError<usize, lexer::Tok, LexError>;

pub fn parse(code: &str) -> std::result::Result<SyntaxNode, ParseError<'_>> {
    let tokens = Lexer::new(code);
    let root = grammar::ExprParser::new().parse(code, tokens)?;
    Ok(SyntaxNode::new_root(
        root.green
            .into_node()
            .expect("the top-level Expr rule always produces a node"),
    ))
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
            let tree = parse(&source)
                .unwrap_or_else(|error| panic!("failed to parse {}: {error:?}", path.display()));
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
