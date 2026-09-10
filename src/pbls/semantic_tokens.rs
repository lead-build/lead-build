//! `textDocument/semanticTokens/full`: highlights assignment/binding-target
//! identifiers (the `key` in `key = expr;`, and matcher-introduced binder
//! names) differently from variable-reference identifiers.
//!
//! Built on [`crate::pblang::visit::LangVisitor`]: every node kind just
//! collects the `(TextRange, is_declaration)` entries found in its
//! children, adding its own entry at exactly the handful of grammar
//! positions that introduce or reference a name — `ASSIGNMENT`'s bare-ident
//! key, `MATCHER_IDENT`, `MATCHER_ALIAS`'s own `@ ident`,
//! `OBJECT_MATCHER_FIELD`'s own key, and `VAR_EXPR` (see each `visit_*`
//! below for which). Nothing else needs classifying: those are exactly the
//! grammar positions a binder or reference name can appear in. Entries are
//! collected in whatever order the walk happens to visit them and sorted by
//! position at the end, rather than relied on to already be in document
//! order — `OBJECT_MATCHER_FIELD`'s own key, in particular, is recorded
//! after its nested matcher/default have already been walked, so it would
//! land out of order without the final sort.

use std::convert::Infallible;
use std::ops::Range;

use rowan::TextRange;
use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend,
};

use crate::pblang::syntaxtree::{SyntaxNode, SyntaxToken};
use crate::pblang::visit::{
    AssignKey, AttrSelector, LangVisitor, MapKind, ObjectField, StringPart, walk_expr,
};

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

/// One classified identifier: its span, and whether it's a binding target
/// (`true`) or a variable reference (`false`).
type Entry = (TextRange, bool);

fn concat(mut a: Vec<Entry>, b: Vec<Entry>) -> Vec<Entry> {
    a.extend(b);
    a
}

struct SemanticVisitor;

impl LangVisitor for SemanticVisitor {
    type Expr = Vec<Entry>;
    type Matcher = Vec<Entry>;
    type Error = Infallible;

    fn visit_let(
        &mut self,
        _range: TextRange,
        bindings: Vec<(TextRange, Vec<Entry>, Vec<Entry>)>,
        body: Vec<Entry>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = Vec::new();
        for (_, matcher, value) in bindings {
            out.extend(matcher);
            out.extend(value);
        }
        out.extend(body);
        Ok(out)
    }

    fn visit_bind(
        &mut self,
        _range: TextRange,
        items: Vec<(TextRange, AssignKey, Vec<Entry>)>,
        body: Vec<Entry>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = Vec::new();
        for (_, key, value) in items {
            if let AssignKey::Ident(token) = key {
                out.push((token.text_range(), true));
            }
            out.extend(value);
        }
        out.extend(body);
        Ok(out)
    }

    fn visit_func_def(
        &mut self,
        _range: TextRange,
        params: Vec<Vec<Entry>>,
        body: Vec<Entry>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out: Vec<Entry> = params.into_iter().flatten().collect();
        out.extend(body);
        Ok(out)
    }

    fn visit_binary(
        &mut self,
        _range: TextRange,
        _op: SyntaxToken,
        lhs: Vec<Entry>,
        rhs: Vec<Entry>,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(concat(lhs, rhs))
    }

    fn visit_unary(
        &mut self,
        _range: TextRange,
        _op: SyntaxToken,
        operand: Vec<Entry>,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(operand)
    }

    fn visit_func_call(
        &mut self,
        _range: TextRange,
        func: Vec<Entry>,
        arg: Vec<Entry>,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(concat(func, arg))
    }

    fn visit_attr_sel(
        &mut self,
        _range: TextRange,
        base: Vec<Entry>,
        attr: AttrSelector<Vec<Entry>>,
    ) -> Result<Vec<Entry>, Infallible> {
        let attr = match attr {
            AttrSelector::Dynamic(entries) => entries,
            AttrSelector::Static(_) => Vec::new(),
        };
        Ok(concat(base, attr))
    }

    fn visit_fold(
        &mut self,
        _range: TextRange,
        func: Vec<Entry>,
        init: Option<Vec<Entry>>,
        input: Vec<Entry>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = func;
        out.extend(init.into_iter().flatten());
        out.extend(input);
        Ok(out)
    }

    fn visit_map(
        &mut self,
        _range: TextRange,
        _kind: MapKind,
        func: Vec<Entry>,
        input: Vec<Entry>,
        filter: Option<Vec<Entry>>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = func;
        out.extend(input);
        out.extend(filter.into_iter().flatten());
        Ok(out)
    }

    fn visit_switch(
        &mut self,
        _range: TextRange,
        input: Vec<Entry>,
        cases: Vec<(Vec<Entry>, Vec<Entry>)>,
        default: Option<Vec<Entry>>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = input;
        for (pattern, result) in cases {
            out.extend(pattern);
            out.extend(result);
        }
        out.extend(default.into_iter().flatten());
        Ok(out)
    }

    fn visit_object(
        &mut self,
        _range: TextRange,
        items: Vec<(TextRange, AssignKey, Vec<Entry>)>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = Vec::new();
        for (_, key, value) in items {
            if let AssignKey::Ident(token) = key {
                out.push((token.text_range(), true));
            }
            out.extend(value);
        }
        Ok(out)
    }

    fn visit_list(
        &mut self,
        _range: TextRange,
        items: Vec<Vec<Entry>>,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(items.into_iter().flatten().collect())
    }

    fn visit_tuple(
        &mut self,
        _range: TextRange,
        items: Vec<Vec<Entry>>,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(items.into_iter().flatten().collect())
    }

    fn visit_literal(
        &mut self,
        _range: TextRange,
        _token: SyntaxToken,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(Vec::new())
    }

    fn visit_string(
        &mut self,
        _range: TextRange,
        parts: Vec<StringPart<Vec<Entry>>>,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(parts
            .into_iter()
            .filter_map(|part| match part {
                StringPart::Embed(entries) => Some(entries),
                StringPart::Chunk(_) => None,
            })
            .flatten()
            .collect())
    }

    fn visit_var(
        &mut self,
        _range: TextRange,
        name: SyntaxToken,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(vec![(name.text_range(), false)])
    }

    fn visit_matcher_ident(
        &mut self,
        _range: TextRange,
        name: SyntaxToken,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(vec![(name.text_range(), true)])
    }

    fn visit_matcher_wildcard(&mut self, _range: TextRange) -> Result<Vec<Entry>, Infallible> {
        Ok(Vec::new())
    }

    fn visit_matcher_alias(
        &mut self,
        _range: TextRange,
        inner: Vec<Entry>,
        name: SyntaxToken,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = inner;
        out.push((name.text_range(), true));
        Ok(out)
    }

    fn visit_matcher_tuple(
        &mut self,
        _range: TextRange,
        items: Vec<Vec<Entry>>,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(items.into_iter().flatten().collect())
    }

    fn visit_matcher_object(
        &mut self,
        _range: TextRange,
        _exhaustive: bool,
        fields: Vec<ObjectField<Vec<Entry>, Vec<Entry>>>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = Vec::new();
        for field in fields {
            out.push((field.key.text_range(), true));
            out.extend(field.matcher.into_iter().flatten());
            out.extend(field.default.into_iter().flatten());
        }
        Ok(out)
    }
}

/// Walks the tree, classifying every declaration/reference identifier, and
/// returns them in document order. Pure tree-shape lookup, kept separate
/// from wire-format encoding so it's testable on its own.
fn classify_tokens(node: &SyntaxNode) -> Vec<(Range<usize>, bool)> {
    let mut visitor = SemanticVisitor;
    let mut entries = walk_expr(node, &mut visitor).unwrap();
    entries.sort_by_key(|(range, _)| range.start());
    entries
        .into_iter()
        .map(|(range, is_declaration)| {
            (
                usize::from(range.start())..usize::from(range.end()),
                is_declaration,
            )
        })
        .collect()
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
            token_modifiers_bitset: if *is_declaration {
                MODIFIER_DECLARATION
            } else {
                0
            },
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
        let node = pblang::parse(source).tree.unwrap();
        let classified = classify_tokens(&node);
        let tokens = encode_semantic_tokens(source, &line_index, &classified);
        assert_eq!(tokens.len(), 2);
        // First token ("x" in "    x = null;") is on line 1.
        assert_eq!(tokens[0].delta_line, 1);
        // Second token ("x" in "in x") is on a later line, so delta_start is absolute.
        assert!(tokens[1].delta_line >= 1);
    }
}
