//! `textDocument/definition`: jumps from a variable reference to where it
//! was declared, reusing the same `Scope`-tracking walk as
//! `semantic_tokens.rs` (see [`crate::pbls::semantic_visitor`]) — each
//! resolved `Entry` already carries its declaration's own `TextRange`
//! alongside its `VarKind`, so this only needs to find the entry under the
//! cursor and hand that range back.

use rowan::{TextRange, TextSize};
use tower_lsp::lsp_types::{Position, Range as LspRange};

use crate::pblang::visit::walk_expr;

use super::document::OpenDocument;
use super::semantic_visitor::{Scope, SemanticVisitor, VarKind};

impl OpenDocument {
    /// The declaration a variable reference at `position` resolves to — or,
    /// if `position` lands directly on a declaration, that declaration's
    /// own range (an `Entry`'s declaration-site range always points at
    /// itself in that case — see `semantic_visitor::Entry`). `None` when
    /// the document doesn't currently parse, `position` isn't on any
    /// tracked identifier, or it's a property name or unresolved reference
    /// (neither of which `Scope` tracks).
    pub fn goto_definition(&self, position: Position) -> Option<LspRange> {
        let node = self.syntax_node()?;
        // Needs the declaration site itself, not just which `VarKind` —
        // `SemanticVisitor<(VarKind, TextRange)>` carries both.
        let mut visitor = SemanticVisitor::<(VarKind, TextRange)>::default();
        let entries = walk_expr(&node, &mut visitor, &Scope::default()).unwrap();
        let offset = TextSize::try_from(self.line_index.offset(&self.text, position)).ok()?;
        let (_, _, kind) = entries
            .into_iter()
            .find(|(range, _, _)| range.contains_inclusive(offset))?;
        let (_, decl_range) = kind?;
        let span = usize::from(decl_range.start())..usize::from(decl_range.end());
        Some(self.line_index.range(&self.text, span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition_at(source: &str, position: Position) -> Option<LspRange> {
        OpenDocument::new(source.to_string()).goto_definition(position)
    }

    #[test]
    fn reference_resolves_to_let_binding() {
        // "let x = 1; in x" — the reference "x" (byte 15) resolves to the
        // binder "x" (bytes 4..5).
        let source = "let x = 1; in x";
        let position = Position {
            line: 0,
            character: 15,
        };
        let range = definition_at(source, position).expect("resolves");
        assert_eq!(range.start, Position::new(0, 4));
        assert_eq!(range.end, Position::new(0, 5));
    }

    #[test]
    fn reference_resolves_to_func_arg() {
        let source = "|x| x";
        let position = Position {
            line: 0,
            character: 4,
        };
        let range = definition_at(source, position).expect("resolves");
        assert_eq!(range.start, Position::new(0, 1));
        assert_eq!(range.end, Position::new(0, 2));
    }

    #[test]
    fn reference_resolves_to_bind_item() {
        let source = "bind x = 1; in x";
        let position = Position {
            line: 0,
            character: 15,
        };
        let range = definition_at(source, position).expect("resolves");
        assert_eq!(range.start, Position::new(0, 5));
        assert_eq!(range.end, Position::new(0, 6));
    }

    #[test]
    fn clicking_a_declaration_resolves_to_itself() {
        let source = "let x = 1; in x";
        let position = Position {
            line: 0,
            character: 4,
        };
        let range = definition_at(source, position).expect("resolves");
        assert_eq!(range.start, Position::new(0, 4));
        assert_eq!(range.end, Position::new(0, 5));
    }

    #[test]
    fn unresolved_reference_has_no_definition() {
        let range = definition_at("x", Position::new(0, 0));
        assert_eq!(range, None);
    }

    #[test]
    fn object_literal_shorthand_resolves_to_the_referenced_variable() {
        // `{ x; }` is sugar for `{ x = x; }` — goto-definition on the
        // shorthand token jumps to the outer `x`'s declaration, unlike a
        // plain property key.
        let source = "bind x = 1; in { x; }";
        let position = Position {
            line: 0,
            character: 17,
        };
        let range = definition_at(source, position).expect("resolves");
        assert_eq!(range.start, Position::new(0, 5));
        assert_eq!(range.end, Position::new(0, 6));
    }

    #[test]
    fn object_literal_key_has_no_definition() {
        let range = definition_at("{ x = 1; }", Position::new(0, 2));
        assert_eq!(range, None);
    }

    #[test]
    fn static_attr_selector_has_no_definition() {
        let source = "let x = null; in x.foo";
        let position = Position {
            line: 0,
            character: 20,
        };
        assert_eq!(definition_at(source, position), None);
    }

    #[test]
    fn position_outside_any_identifier_has_no_definition() {
        let range = definition_at("let x = 1; in x", Position::new(0, 9));
        assert_eq!(range, None);
    }
}
