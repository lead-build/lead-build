//! `textDocument/semanticTokens/full`: highlights assignment/binding-target
//! identifiers (the `key` in `key = expr;`, and matcher-introduced binder
//! names) differently from variable-reference identifiers.
//!
//! Classification is purely by the `IDENT` token's immediate parent
//! `SyntaxKind` — see `syntaxtree.rs`'s `ASSIGNMENT`/`MATCHER_IDENT`/
//! `MATCHER_ALIAS`/`OBJECT_MATCHER_FIELD`/`VAR_EXPR` doc comments. No binding
//! resolution is needed: the grammar never re-wraps a binder ident in
//! anything else, and a reference is always wrapped in `VAR_EXPR`.

use std::ops::Range;

use rowan::NodeOrToken;
use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend,
};

use crate::pblang::syntaxtree::{SyntaxKind, SyntaxNode};

use super::convert::LineIndex;
use super::document::OpenDocument;

pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![SemanticTokenType::VARIABLE],
        token_modifiers: vec![SemanticTokenModifier::DECLARATION],
    }
}

const TOKEN_TYPE_VARIABLE: u32 = 0;
const MODIFIER_DECLARATION: u32 = 1 << 0;

/// Whether an `IDENT` token whose immediate parent has this `SyntaxKind` is a
/// binding target (`Some(true)`), a variable reference (`Some(false)`), or
/// neither (`None` — not every `IDENT` in the tree is one of the two; e.g. an
/// `OBJECT_MATCHER_FIELD`'s own `key` vs. its nested `matcher` share a parent
/// only at the field level, already covered here).
fn classify(parent_kind: SyntaxKind) -> Option<bool> {
    match parent_kind {
        SyntaxKind::ASSIGNMENT
        | SyntaxKind::MATCHER_IDENT
        | SyntaxKind::MATCHER_ALIAS
        | SyntaxKind::OBJECT_MATCHER_FIELD => Some(true),
        SyntaxKind::VAR_EXPR => Some(false),
        _ => None,
    }
}

/// Walks the tree in document order, classifying every `IDENT` token. Pure
/// tree-shape lookup, kept separate from wire-format encoding so it's
/// testable on its own.
fn classify_tokens(node: &SyntaxNode) -> Vec<(Range<usize>, bool)> {
    let mut classified = Vec::new();
    for element in node.descendants_with_tokens() {
        let NodeOrToken::Token(token) = element else {
            continue;
        };
        if token.kind() != SyntaxKind::IDENT {
            continue;
        }
        let Some(parent) = token.parent() else {
            continue;
        };
        if let Some(is_declaration) = classify(parent.kind()) {
            let range = token.text_range();
            classified.push((usize::from(range.start())..usize::from(range.end()), is_declaration));
        }
    }
    classified
}

/// Delta-encodes classified spans (already in document order) into the
/// wire format `textDocument/semanticTokens/full` expects.
fn encode_semantic_tokens(
    source: &str,
    line_index: &LineIndex,
    classified: &[(Range<usize>, bool)],
) -> Vec<SemanticToken> {
    let mut tokens = Vec::with_capacity(classified.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
    for (span, is_declaration) in classified {
        let start = line_index.position(source, span.start);
        let length = source[span.clone()].encode_utf16().count() as u32;
        let delta_line = start.line - prev_line;
        let delta_start = if delta_line == 0 {
            start.character - prev_start
        } else {
            start.character
        };
        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type: TOKEN_TYPE_VARIABLE,
            token_modifiers_bitset: if *is_declaration { MODIFIER_DECLARATION } else { 0 },
        });
        prev_line = start.line;
        prev_start = start.character;
    }
    tokens
}

impl OpenDocument {
    /// `None` when the document's latest parse failed — nothing to
    /// highlight until it parses again.
    pub fn semantic_tokens(&self) -> Option<Vec<SemanticToken>> {
        let node = self.syntax_node()?;
        Some(encode_semantic_tokens(
            &self.text,
            &self.line_index,
            &classify_tokens(&node),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pblang;

    fn tokens_for(source: &str) -> Vec<SemanticToken> {
        OpenDocument::new(source.to_string())
            .semantic_tokens()
            .expect("valid source")
    }

    #[test]
    fn let_binding_target_and_reference_are_told_apart() {
        let source = "let x = null; in x";
        let tokens = tokens_for(source);
        // "x" (binder, in `let x = ...`), then "x" (reference, in the body).
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token_modifiers_bitset, MODIFIER_DECLARATION);
        assert_eq!(tokens[1].token_modifiers_bitset, 0);
        assert_eq!(tokens[0].token_type, TOKEN_TYPE_VARIABLE);
        assert_eq!(tokens[1].token_type, TOKEN_TYPE_VARIABLE);
    }

    #[test]
    fn assignment_key_and_value_reference_are_told_apart() {
        let source = "bind y = null; in { x = y; }";
        let tokens = tokens_for(source);
        // "y" (binder, `bind y = ...`), "x" (key, `x = y;`), "y" (reference, value).
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].token_modifiers_bitset, MODIFIER_DECLARATION); // bind target y
        assert_eq!(tokens[1].token_modifiers_bitset, MODIFIER_DECLARATION); // object key x
        assert_eq!(tokens[2].token_modifiers_bitset, 0); // reference to y
    }

    #[test]
    fn encoding_deltas_are_relative_to_previous_token() {
        let source = "let\n    x = null;\nin x";
        let line_index = LineIndex::new(source);
        let node = pblang::parse(source).unwrap();
        let classified = classify_tokens(&node);
        let tokens = encode_semantic_tokens(source, &line_index, &classified);
        assert_eq!(tokens.len(), 2);
        // First token ("x" in "    x = null;") is on line 1.
        assert_eq!(tokens[0].delta_line, 1);
        // Second token ("x" in "in x") is on a later line, so delta_start is absolute.
        assert!(tokens[1].delta_line >= 1);
    }
}
