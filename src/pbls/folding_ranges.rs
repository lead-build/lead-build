//! `textDocument/foldingRange`: every multi-line "container" node (object,
//! bind/let block, switch, list, matcher-object) becomes a fold candidate.
//! Purely mechanical — no classification beyond "is this the kind of node an
//! editor would reasonably want to fold" and "does it actually span more
//! than one line".

use tower_lsp::lsp_types::FoldingRange;

use crate::pblang::syntaxtree::{SyntaxKind, SyntaxNode};

use super::convert::LineIndex;

fn is_foldable_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::OBJECT_EXPR
            | SyntaxKind::BIND_EXPR
            | SyntaxKind::LET_EXPR
            | SyntaxKind::SWITCH_EXPR
            | SyntaxKind::MAP_EXPR
            | SyntaxKind::MATCHER_OBJECT
            | SyntaxKind::LIST_EXPR
            | SyntaxKind::TUPLE_EXPR
    )
}

pub fn folding_ranges_for_source(
    node: &SyntaxNode,
    line_index: &LineIndex,
    source: &str,
) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    for descendant in node.descendants() {
        if !is_foldable_kind(descendant.kind()) {
            continue;
        }
        let span = descendant.text_range().into();
        let range = line_index.range(source, span);
        if range.end.line > range.start.line {
            ranges.push(FoldingRange {
                start_line: range.start.line,
                end_line: range.end.line,
                ..FoldingRange::default()
            });
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pblang;

    fn ranges_for(source: &str) -> Vec<FoldingRange> {
        let node = pblang::parse(source).expect("valid source");
        let line_index = LineIndex::new(source);
        folding_ranges_for_source(&node, &line_index, source)
    }

    #[test]
    fn multiline_object_is_foldable() {
        let source = "{\n    a = null;\n}";
        let ranges = ranges_for(source);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start_line, 0);
        assert_eq!(ranges[0].end_line, 2);
    }

    #[test]
    fn single_line_object_is_not_foldable() {
        let ranges = ranges_for("{ a = null; }");
        assert_eq!(ranges, Vec::new());
    }

    #[test]
    fn nested_multiline_blocks_each_get_a_range() {
        let source = "let\n    x = {\n        a = null;\n    };\nin x";
        let ranges = ranges_for(source);
        // The outer LET_EXPR and the inner OBJECT_EXPR are both multi-line.
        assert_eq!(ranges.len(), 2);
    }
}
