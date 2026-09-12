//! `textDocument/rename` and `textDocument/prepareRename`: renames a
//! variable or function parameter across every place it's used. A rename
//! is exactly "replace every reference plus the declaration with a new
//! name", which is what `find_references.rs` already computes — this just
//! delegates to it (always including the declaration) rather than
//! re-walking the tree. Properties (object-literal keys, matcher rename
//! keys, static `ATTR_SEL` names) and unresolved references are refused:
//! neither has a `Scope`-tracked declaration to safely rename together
//! with, and `find_references`/the entry classification already treat both
//! as unresolved (`kind: None`).

use rowan::{TextRange, TextSize};
use tower_lsp::lsp_types::{Position, Range as LspRange};

use crate::pblang::visit::walk_expr;

use super::document::OpenDocument;
use super::semantic_visitor::{Scope, SemanticVisitor};

impl OpenDocument {
    /// The range of the identifier at `position`, if it's something rename
    /// can act on (a variable or parameter) — `None` for a property name,
    /// an unresolved reference, or if the document doesn't parse. Backs
    /// `textDocument/prepareRename`, so editors can gray out rename
    /// entirely instead of only failing after the user has typed a new
    /// name.
    pub fn prepare_rename(&self, position: Position) -> Option<LspRange> {
        let node = self.syntax_node()?;
        let mut visitor = SemanticVisitor::<TextRange>::default();
        let entries = walk_expr(&node, &mut visitor, &Scope::default()).unwrap();
        let offset = TextSize::try_from(self.line_index.offset(&self.text, position)).ok()?;
        let (range, _, kind) = entries
            .into_iter()
            .find(|(range, _, _)| range.contains_inclusive(offset))?;
        kind?;
        let span = usize::from(range.start())..usize::from(range.end());
        Some(self.line_index.range(&self.text, span))
    }

    /// Every range that must change to rename the symbol at `position` —
    /// its declaration plus every reference to it. `None` for exactly the
    /// same reasons as `prepare_rename`.
    pub fn rename(&self, position: Position) -> Option<Vec<LspRange>> {
        self.find_references(position, true)
    }
}

/// Whether `name` is a syntactically valid identifier in this language —
/// same shape as the lexer's `IDENT` token (`src/pblang/lexer.rs`:
/// `[a-zA-Z][a-zA-Z0-9_]*`). Used to reject a rename before it would
/// produce a `WorkspaceEdit` that corrupts the source with an unparseable
/// name.
pub fn is_valid_new_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepare_rename_at(source: &str, position: Position) -> Option<LspRange> {
        OpenDocument::new(source.to_string()).prepare_rename(position)
    }

    fn rename_at(source: &str, position: Position) -> Option<Vec<LspRange>> {
        OpenDocument::new(source.to_string()).rename(position)
    }

    fn starts(ranges: &[LspRange]) -> Vec<u32> {
        ranges.iter().map(|r| r.start.character).collect()
    }

    #[test]
    fn let_bound_variable_renames_declaration_and_every_reference() {
        // "let x = 1; in x + x" — declaration at 4, references at 14 and 18.
        let source = "let x = 1; in x + x";
        let ranges = rename_at(source, Position::new(0, 4)).expect("renamable");
        assert_eq!(starts(&ranges), vec![4, 14, 18]);
    }

    #[test]
    fn func_arg_renames_param_and_its_uses() {
        let source = "|x| x + x";
        let ranges = rename_at(source, Position::new(0, 1)).expect("renamable");
        assert_eq!(starts(&ranges), vec![1, 4, 8]);
    }

    #[test]
    fn clicking_a_reference_still_renames_the_whole_symbol() {
        let source = "let x = 1; in x + x";
        let ranges = rename_at(source, Position::new(0, 14)).expect("renamable");
        assert_eq!(starts(&ranges), vec![4, 14, 18]);
    }

    #[test]
    fn object_literal_key_is_not_renamable() {
        let source = "{ x = 1; }";
        let position = Position::new(0, 2);
        assert_eq!(prepare_rename_at(source, position), None);
        assert_eq!(rename_at(source, position), None);
    }

    #[test]
    fn matcher_rename_key_is_not_renamable() {
        // `{a = b}`: "a" is a property key, not a variable — "b" (the
        // bound pattern) still is.
        let source = "let f = |{a = b}| b; in f";
        let key_position = Position::new(0, 10);
        assert_eq!(prepare_rename_at(source, key_position), None);
        assert_eq!(rename_at(source, key_position), None);
    }

    #[test]
    fn static_attr_selector_is_not_renamable() {
        let source = "let x = null; in x.foo";
        let position = Position::new(0, 19);
        assert_eq!(prepare_rename_at(source, position), None);
        assert_eq!(rename_at(source, position), None);
    }

    #[test]
    fn unresolved_reference_is_not_renamable() {
        let position = Position::new(0, 0);
        assert_eq!(prepare_rename_at("x", position), None);
        assert_eq!(rename_at("x", position), None);
    }

    #[test]
    fn valid_new_names_are_accepted() {
        assert!(is_valid_new_name("x"));
        assert!(is_valid_new_name("myVar"));
        assert!(is_valid_new_name("my_var_2"));
    }

    #[test]
    fn invalid_new_names_are_rejected() {
        assert!(!is_valid_new_name(""));
        assert!(!is_valid_new_name("2x"));
        assert!(!is_valid_new_name("my var"));
        assert!(!is_valid_new_name("my-var"));
    }
}
