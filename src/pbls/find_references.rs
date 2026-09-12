//! `textDocument/references`: lists every location a symbol is used at —
//! the mirror image of `goto_definition.rs`. Where goto-definition asks
//! "where was this declared", find-references asks "which entries resolve
//! to the same declaration as this one" — the same `Scope`-tracking walk
//! (see [`crate::pbls::semantic_visitor`]), just grouped differently. This
//! never needs `VarKind`, only the plain declaration-site `TextRange` to
//! group by, so it instantiates `SemanticVisitor<TextRange>` rather than
//! reusing goto-definition's `(VarKind, TextRange)`.

use rowan::{TextRange, TextSize};
use tower_lsp::lsp_types::{Position, Range as LspRange};

use crate::pblang::visit::walk_expr;

use super::document::OpenDocument;
use super::semantic_visitor::{Scope, SemanticVisitor};

impl OpenDocument {
    /// Every location the symbol at `position` is used at: its own
    /// declaration (when `include_declaration`) plus every reference that
    /// resolves to the same declaration, in document order. `None` when the
    /// document doesn't currently parse, or `position` isn't on a tracked
    /// identifier (a property name or an unresolved reference — neither of
    /// which `Scope` tracks). `Some(vec![])` is a real, empty result (e.g.
    /// an unreferenced declaration with `include_declaration: false`),
    /// distinct from `None`.
    pub fn find_references(
        &self,
        position: Position,
        include_declaration: bool,
    ) -> Option<Vec<LspRange>> {
        let node = self.syntax_node()?;
        let mut visitor = SemanticVisitor::<TextRange>::default();
        let entries = walk_expr(&node, &mut visitor, &Scope::default()).unwrap();
        let offset = TextSize::try_from(self.line_index.offset(&self.text, position)).ok()?;
        // A declaration entry's own resolved `TextRange` always points at
        // itself (see `semantic_visitor::Entry`), so this works whether the
        // cursor landed on the declaration or on any one reference.
        let target = entries
            .iter()
            .find(|(range, _, _)| range.contains_inclusive(offset))?
            .2?;
        let mut ranges: Vec<LspRange> = entries
            .into_iter()
            .filter(|(_, is_declaration, decl)| {
                *decl == Some(target) && (include_declaration || !is_declaration)
            })
            .map(|(range, ..)| {
                let span = usize::from(range.start())..usize::from(range.end());
                self.line_index.range(&self.text, span)
            })
            .collect();
        ranges.sort_by_key(|range| (range.start.line, range.start.character));
        Some(ranges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn references_at(
        source: &str,
        position: Position,
        include_declaration: bool,
    ) -> Option<Vec<LspRange>> {
        OpenDocument::new(source.to_string()).find_references(position, include_declaration)
    }

    fn starts(ranges: &[LspRange]) -> Vec<u32> {
        ranges.iter().map(|r| r.start.character).collect()
    }

    #[test]
    fn let_bound_variable_lists_declaration_and_every_reference() {
        // "let x = 1; in x + x" — declaration at 4, references at 14 and 18.
        let source = "let x = 1; in x + x";
        let ranges = references_at(source, Position::new(0, 4), true).expect("resolves");
        assert_eq!(starts(&ranges), vec![4, 14, 18]);
    }

    #[test]
    fn include_declaration_false_excludes_the_declaration() {
        let source = "let x = 1; in x + x";
        let ranges = references_at(source, Position::new(0, 4), false).expect("resolves");
        assert_eq!(starts(&ranges), vec![14, 18]);
    }

    #[test]
    fn clicking_a_reference_finds_every_occurrence() {
        let source = "let x = 1; in x + x";
        // Click on the reference at character 14, not the declaration.
        let ranges = references_at(source, Position::new(0, 14), true).expect("resolves");
        assert_eq!(starts(&ranges), vec![4, 14, 18]);
    }

    #[test]
    fn func_arg_lists_param_and_its_uses() {
        let source = "|x| x + x";
        let ranges = references_at(source, Position::new(0, 1), true).expect("resolves");
        assert_eq!(starts(&ranges), vec![1, 4, 8]);
    }

    #[test]
    fn unreferenced_declaration_without_declaration_is_empty() {
        let source = "let x = 1; in null";
        let ranges = references_at(source, Position::new(0, 4), false).expect("resolves");
        assert_eq!(ranges, Vec::new());
    }

    #[test]
    fn unresolved_reference_has_no_references() {
        assert_eq!(references_at("x", Position::new(0, 0), true), None);
    }

    #[test]
    fn object_literal_shorthand_counts_as_a_reference() {
        // "bind x = 1; in { x; }" — declaration at 5, shorthand reference at 17.
        let source = "bind x = 1; in { x; }";
        let ranges = references_at(source, Position::new(0, 5), true).expect("resolves");
        assert_eq!(starts(&ranges), vec![5, 17]);
    }

    #[test]
    fn property_name_has_no_references() {
        assert_eq!(references_at("{ x = 1; }", Position::new(0, 2), true), None);
    }
}
