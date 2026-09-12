//! `textDocument/rename` and `textDocument/prepareRename`: renames a
//! variable or function parameter across every place it's used. Properties
//! (object-literal keys, matcher rename keys, static `ATTR_SEL` names) and
//! unresolved references are refused: neither has a `Scope`-tracked
//! declaration to safely rename together with, and the entry classification
//! already treats both as unresolved (`kind: None`).
//!
//! One case needs more than a plain text substitution: an object-matcher
//! **shorthand** field (`{foo}` in `|{foo}| foo`) uses one token as *both*
//! the property name being matched *and* the local variable name. Renaming
//! it in place would silently match a different property. Per
//! `src/pblang/grammar.lalrpop:396-405`:
//! ```text
//! ObjectMatcher: GreenSpan = {
//!     <key:Ident> => OBJECT_MATCHER_FIELD[key],                                  // shorthand
//!     <key:Ident> <eq:T_Eq> <matcher:Matcher> => OBJECT_MATCHER_FIELD[key, eq, matcher],  // explicit
//! };
//! ```
//! - Shorthand's `key` is a raw `IDENT` token that is a *direct token
//!   child* of `OBJECT_MATCHER_FIELD` (no wrapping node) — renaming it must
//!   **expand** the field to `key = newName`, keeping the original
//!   property name.
//! - Explicit form's pattern is `MATCHER_IDENT` (directly parented by
//!   `OBJECT_MATCHER_FIELD`) only when it's a bare identifier — renaming
//!   that pattern to text matching the property key **collapses** it back
//!   to shorthand `key`. Anything nested (`{foo = (x,)}`, `{foo = x @ _}`,
//!   `{foo = {bar}}`) has an extra node in between (`MATCHER_TUPLE`,
//!   `MATCHER_ALIAS`, `MATCHER_OBJECT`), so this check doesn't misfire on
//!   those — they always get a plain in-place rename.
//!
//! Every reference elsewhere in the body is an ordinary variable use and
//! renames in place regardless of which of these cases the declaration is.

use rowan::{TextRange, TextSize};
use tower_lsp::lsp_types::{Position, Range as LspRange, TextEdit};

use crate::pblang::{SyntaxKind, SyntaxNode, visit::walk_expr};

use super::document::OpenDocument;
use super::semantic_visitor::{Scope, SemanticVisitor};

impl OpenDocument {
    /// The range of the identifier at `position`, if it's something rename
    /// can act on (a variable or parameter) — `None` for a property name,
    /// an unresolved reference, or if the document doesn't parse. Backs
    /// `textDocument/prepareRename`, so editors can gray out rename
    /// entirely instead of only failing after the user has typed a new
    /// name. Unaffected by the shorthand/explicit distinction below: it
    /// only reports *where* the renamable token is, not how `rename` will
    /// end up editing it.
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

    /// Every edit needed to rename the symbol at `position` to `new_name`
    /// — its declaration plus every reference to it. `None` for exactly
    /// the same reasons as `prepare_rename`.
    pub fn rename(&self, position: Position, new_name: &str) -> Option<Vec<TextEdit>> {
        let node = self.syntax_node()?;
        let mut visitor = SemanticVisitor::<TextRange>::default();
        let entries = walk_expr(&node, &mut visitor, &Scope::default()).unwrap();
        let offset = TextSize::try_from(self.line_index.offset(&self.text, position)).ok()?;
        let target = entries
            .iter()
            .find(|(range, _, _)| range.contains_inclusive(offset))?
            .2?;
        let mut edits: Vec<TextEdit> = entries
            .into_iter()
            .filter(|(_, _, decl)| *decl == Some(target))
            .filter_map(|(range, is_declaration, _)| {
                if is_declaration {
                    self.declaration_rename_edit(&node, range, new_name)
                } else {
                    self.reference_rename_edit(&node, range, new_name)
                }
            })
            .collect();
        edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
        Some(edits)
    }

    /// The edit for a declaration site specifically — usually a plain
    /// in-place replacement, except for an object-matcher field where the
    /// same token also names a property (see the module doc). `None` means
    /// no edit is needed at all (a shorthand field renamed to its own
    /// current text — already in its simplest form).
    fn declaration_rename_edit(
        &self,
        node: &SyntaxNode,
        range: TextRange,
        new_name: &str,
    ) -> Option<TextEdit> {
        let token = node
            .descendants_with_tokens()
            .filter_map(|el| el.into_token())
            .find(|t| t.text_range() == range)?;
        let parent = token.parent()?;
        if parent.kind() == SyntaxKind::OBJECT_MATCHER_FIELD {
            // Shorthand `{foo}`: `token` is both the property key and the
            // bound name.
            let key_text = token.text();
            return if new_name == key_text {
                None
            } else {
                Some(self.text_edit(range, format!("{key_text} = {new_name}")))
            };
        }
        if parent.kind() == SyntaxKind::MATCHER_IDENT
            && parent
                .parent()
                .is_some_and(|gp| gp.kind() == SyntaxKind::OBJECT_MATCHER_FIELD)
        {
            // Explicit `{foo = old}` where `old` is a bare ident: `parent`
            // is the field itself's `MATCHER_IDENT` child.
            let field = parent.parent()?;
            let key = field
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .find(|t| t.kind() == SyntaxKind::IDENT)?;
            if new_name == key.text() {
                let collapsed = TextRange::new(key.text_range().start(), range.end());
                return Some(self.text_edit(collapsed, key.text().to_string()));
            }
        }
        Some(self.text_edit(range, new_name.to_string()))
    }

    /// The edit for a reference site specifically — usually a plain
    /// in-place replacement, except where the reference is the implicit
    /// value of an object-literal shorthand `ASSIGNMENT` (`{ foo; }`), or
    /// its full-form counterpart (`{ foo = bar; }`), which share a token
    /// with a property name and need the same expand/collapse treatment as
    /// `declaration_rename_edit`'s object-matcher-field case, just applied
    /// to `ASSIGNMENT` instead of `OBJECT_MATCHER_FIELD`.
    fn reference_rename_edit(
        &self,
        node: &SyntaxNode,
        range: TextRange,
        new_name: &str,
    ) -> Option<TextEdit> {
        let token = node
            .descendants_with_tokens()
            .filter_map(|el| el.into_token())
            .find(|t| t.text_range() == range)?;
        let parent = token.parent()?;

        // Shorthand `foo;` — renaming the variable must expand to full form
        // so the property keeps its original name. Shorthand ASSIGNMENT has
        // no `EQ` child token at all (just IDENT + SEMICOLON). Renaming to
        // the same name needs no edit at all — it's already in its
        // simplest form.
        if parent.kind() == SyntaxKind::ASSIGNMENT
            && !parent
                .children_with_tokens()
                .any(|el| el.kind() == SyntaxKind::EQ)
        {
            let key_text = token.text();
            return if new_name == key_text {
                None
            } else {
                Some(self.text_edit(range, format!("{key_text} = {new_name}")))
            };
        }

        // Full form `key = value;` where value is a bare variable
        // reference — renaming it to match the property key collapses back
        // to shorthand. `token.parent()` is `VAR_EXPR` only when the
        // reference is the whole value (not nested inside a larger
        // expression), so this never misfires on e.g. `key = value + 1;`.
        if parent.kind() == SyntaxKind::VAR_EXPR
            && let Some(assignment) = parent.parent()
            && assignment.kind() == SyntaxKind::ASSIGNMENT
        {
            let key_token = assignment
                .children_with_tokens()
                .filter_map(|el| el.into_token())
                .find(|t| t.kind() == SyntaxKind::IDENT);
            if let Some(key_token) = key_token
                && key_token.text_range() != range
                && key_token.text() == new_name
            {
                let collapsed = TextRange::new(key_token.text_range().start(), range.end());
                return Some(self.text_edit(collapsed, new_name.to_string()));
            }
        }

        Some(self.text_edit(range, new_name.to_string()))
    }

    fn text_edit(&self, range: TextRange, new_text: String) -> TextEdit {
        let span = usize::from(range.start())..usize::from(range.end());
        TextEdit {
            range: self.line_index.range(&self.text, span),
            new_text,
        }
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

    fn rename_at(source: &str, position: Position, new_name: &str) -> Option<Vec<TextEdit>> {
        OpenDocument::new(source.to_string()).rename(position, new_name)
    }

    fn starts(edits: &[TextEdit]) -> Vec<u32> {
        edits.iter().map(|e| e.range.start.character).collect()
    }

    #[test]
    fn let_bound_variable_renames_declaration_and_every_reference() {
        // "let x = 1; in x + x" — declaration at 4, references at 14 and 18.
        let source = "let x = 1; in x + x";
        let edits = rename_at(source, Position::new(0, 4), "y").expect("renamable");
        assert_eq!(starts(&edits), vec![4, 14, 18]);
        assert!(edits.iter().all(|e| e.new_text == "y"));
    }

    #[test]
    fn func_arg_renames_param_and_its_uses() {
        let source = "|x| x + x";
        let edits = rename_at(source, Position::new(0, 1), "y").expect("renamable");
        assert_eq!(starts(&edits), vec![1, 4, 8]);
        assert!(edits.iter().all(|e| e.new_text == "y"));
    }

    #[test]
    fn clicking_a_reference_still_renames_the_whole_symbol() {
        let source = "let x = 1; in x + x";
        let edits = rename_at(source, Position::new(0, 14), "y").expect("renamable");
        assert_eq!(starts(&edits), vec![4, 14, 18]);
    }

    #[test]
    fn shorthand_matcher_field_expands_to_explicit_form() {
        // "|{foo}| foo" — declaration (the shorthand field) at 2, reference
        // at 8. Renaming "foo" to "bar" must keep matching property "foo".
        let source = "|{foo}| foo";
        let edits = rename_at(source, Position::new(0, 2), "bar").expect("renamable");
        assert_eq!(edits.len(), 2);
        let decl = edits.iter().find(|e| e.range.start.character == 2).unwrap();
        assert_eq!(decl.new_text, "foo = bar");
        let reference = edits.iter().find(|e| e.range.start.character == 8).unwrap();
        assert_eq!(reference.new_text, "bar");
    }

    #[test]
    fn shorthand_matcher_field_renamed_to_itself_is_a_no_op_declaration_edit() {
        let source = "|{foo}| foo";
        let edits = rename_at(source, Position::new(0, 2), "foo").expect("renamable");
        // Only the reference gets an (identity) edit; the declaration,
        // already in its simplest form, needs none.
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.character, 8);
    }

    #[test]
    fn object_literal_shorthand_expands_when_its_variable_is_renamed() {
        // "bind foo = 1; in { foo; }" — declaration at 5, shorthand
        // reference at 19. Renaming "foo" to "bar" must keep the object's
        // property named "foo" while updating which variable it reads.
        let source = "bind foo = 1; in { foo; }";
        let edits = rename_at(source, Position::new(0, 5), "bar").expect("renamable");
        assert_eq!(edits.len(), 2);
        let decl = edits.iter().find(|e| e.range.start.character == 5).unwrap();
        assert_eq!(decl.new_text, "bar");
        let reference = edits
            .iter()
            .find(|e| e.range.start.character == 19)
            .unwrap();
        assert_eq!(reference.new_text, "foo = bar");
    }

    #[test]
    fn object_literal_shorthand_renamed_to_itself_is_a_no_op_reference_edit() {
        let source = "bind foo = 1; in { foo; }";
        let edits = rename_at(source, Position::new(0, 5), "foo").expect("renamable");
        // Only the declaration gets an (identity) edit; the shorthand
        // reference, already in its simplest form, needs none.
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.character, 5);
    }

    #[test]
    fn explicit_object_assignment_collapses_when_value_is_renamed_to_match_key() {
        // "bind bar = 1; in { foo = bar; }" — renaming "bar" to "foo"
        // collapses the full form down to shorthand "foo;".
        let source = "bind bar = 1; in { foo = bar; }";
        let edits = rename_at(source, Position::new(0, 5), "foo").expect("renamable");
        assert_eq!(edits.len(), 2);
        let decl = edits.iter().find(|e| e.range.start.character == 5).unwrap();
        assert_eq!(decl.new_text, "foo");
        let reference = edits
            .iter()
            .find(|e| e.range.start.character == 19)
            .unwrap();
        assert_eq!(reference.new_text, "foo");
        assert_eq!(reference.range.end.character, 28);
    }

    #[test]
    fn explicit_matcher_field_renamed_to_new_name_keeps_property() {
        // "|{foo = old}| old" — declaration ("old") at 8, reference at 14.
        let source = "|{foo = old}| old";
        let edits = rename_at(source, Position::new(0, 8), "new").expect("renamable");
        assert_eq!(edits.len(), 2);
        let decl = edits.iter().find(|e| e.range.start.character == 8).unwrap();
        assert_eq!(decl.new_text, "new");
        assert_eq!(decl.range.end.character, 11);
        let reference = edits
            .iter()
            .find(|e| e.range.start.character == 14)
            .unwrap();
        assert_eq!(reference.new_text, "new");
    }

    #[test]
    fn explicit_matcher_field_renamed_to_property_name_collapses_to_shorthand() {
        // "|{foo = old}| old" — renaming "old" to "foo" collapses
        // "foo = old" (chars 2..11) down to just "foo".
        let source = "|{foo = old}| old";
        let edits = rename_at(source, Position::new(0, 8), "foo").expect("renamable");
        assert_eq!(edits.len(), 2);
        let decl = edits
            .iter()
            .find(|e| e.range.start.character == 2)
            .expect("collapsed edit starts at the property key");
        assert_eq!(decl.new_text, "foo");
        assert_eq!(decl.range.end.character, 11);
        let reference = edits
            .iter()
            .find(|e| e.range.start.character == 14)
            .unwrap();
        assert_eq!(reference.new_text, "foo");
    }

    #[test]
    fn nested_matcher_pattern_never_collapses() {
        // "|{foo = (x,)}| x" — "x" is nested inside a MATCHER_TUPLE (a
        // single-element tuple pattern needs the trailing comma), not a
        // direct MATCHER_IDENT child of the field, so even renaming it to
        // "foo" must stay a plain in-place rename, not a collapse.
        let source = "|{foo = (x,)}| x";
        let edits = rename_at(source, Position::new(0, 9), "foo").expect("renamable");
        assert_eq!(edits.len(), 2);
        let decl = edits.iter().find(|e| e.range.start.character == 9).unwrap();
        assert_eq!(decl.new_text, "foo");
        assert_eq!(decl.range.end.character, 10);
    }

    #[test]
    fn object_literal_key_is_not_renamable() {
        let source = "{ x = 1; }";
        let position = Position::new(0, 2);
        assert_eq!(prepare_rename_at(source, position), None);
        assert_eq!(rename_at(source, position, "y"), None);
    }

    #[test]
    fn matcher_rename_key_is_not_renamable() {
        // `{a = b}`: "a" is a property key, not a variable — "b" (the
        // bound pattern) still is.
        let source = "let f = |{a = b}| b; in f";
        let key_position = Position::new(0, 10);
        assert_eq!(prepare_rename_at(source, key_position), None);
        assert_eq!(rename_at(source, key_position, "y"), None);
    }

    #[test]
    fn static_attr_selector_is_not_renamable() {
        let source = "let x = null; in x.foo";
        let position = Position::new(0, 19);
        assert_eq!(prepare_rename_at(source, position), None);
        assert_eq!(rename_at(source, position, "y"), None);
    }

    #[test]
    fn unresolved_reference_is_not_renamable() {
        let position = Position::new(0, 0);
        assert_eq!(prepare_rename_at("x", position), None);
        assert_eq!(rename_at("x", position, "y"), None);
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
