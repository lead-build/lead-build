//! The pb auto-formatter (`pbfmt`). Kept as its own submodule, separate
//! from the lexer/grammar/`syntaxtree`, since those are shared with parsing
//! proper — formatting logic shouldn't leak into them.
//!
//! For now this is just a placeholder: it prints the syntax tree's own text
//! back out unchanged, which is a plain round-trip, not real formatting (and
//! not even a byte-exact round-trip yet — see `syntaxtree`'s docs on why the
//! tree isn't lossless). Real formatting (indentation, line-wrapping,
//! canonicalizing whitespace) replaces the body of [`format_source`] later.

use super::syntaxtree::SyntaxNode;

pub fn format_source(tree: &SyntaxNode) -> String {
    tree.text().to_string()
}
