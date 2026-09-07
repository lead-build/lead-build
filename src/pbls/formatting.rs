//! `textDocument/formatting`: reuses `pblang::fmt::format_tree` (the same
//! formatter `pbfmt` uses) and returns the whole reformatted document as a
//! single `TextEdit` — matching `pbfmt`'s own whole-file formatting.

use tower_lsp::lsp_types::{Position, Range, TextEdit};

use crate::pblang::fmt::format_tree;
use crate::pblang::syntaxtree::SyntaxNode;

use super::convert::LineIndex;

pub fn formatting_edits_for_source(
    source: &str,
    node: &SyntaxNode,
    line_index: &LineIndex,
) -> Vec<TextEdit> {
    let formatted = format_tree(node).text().to_string();
    if formatted == source {
        return Vec::new();
    }
    vec![TextEdit {
        range: Range {
            start: Position { line: 0, character: 0 },
            end: line_index.position(source, source.len()),
        },
        new_text: formatted,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pblang;

    fn edits_for(source: &str) -> Vec<TextEdit> {
        let node = pblang::parse(source).expect("valid source");
        let line_index = LineIndex::new(source);
        formatting_edits_for_source(source, &node, &line_index)
    }

    #[test]
    fn already_formatted_source_produces_no_edits() {
        let formatted = format_tree(&pblang::parse("null").unwrap()).text().to_string();
        assert_eq!(edits_for(&formatted), Vec::new());
    }

    #[test]
    fn poorly_formatted_source_produces_one_edit_covering_the_document() {
        let source = "let   x=null;in x";
        let edits = edits_for(source);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start, Position { line: 0, character: 0 });
        assert_ne!(edits[0].new_text, source);
    }
}
