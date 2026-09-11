//! `textDocument/semanticTokens/full`: highlights assignment/binding-target
//! identifiers (the `key` in `key = expr;`, and matcher-introduced binder
//! names) differently from variable-reference identifiers, and further
//! distinguishes function-parameter variables from `let`/`bind`-bound ones
//! by tracking a lexical [`Scope`] top-down through the walk (see
//! [`crate::pblang::visit::LangVisitor::Down`]).
//!
//! Built on [`crate::pblang::visit::LangVisitor`]: every node kind just
//! collects the `Entry` values found in its children, adding its own entry
//! at exactly the handful of grammar positions that introduce or reference
//! a name — `ASSIGNMENT`'s bare-ident key, `MATCHER_IDENT`,
//! `MATCHER_ALIAS`'s own `@ ident`, `OBJECT_MATCHER_FIELD`'s own key, and
//! `VAR_EXPR` (see each `visit_*` below for which). Nothing else needs
//! classifying: those are exactly the grammar positions a binder or
//! reference name can appear in. Entries are collected in whatever order
//! the walk happens to visit them and sorted by position at the end,
//! rather than relied on to already be in document order — `MATCHER_ALIAS`
//! and `OBJECT_MATCHER_FIELD`'s own name/key, in particular, are recorded
//! after nested matchers have already been walked, so they would land out
//! of order without the final sort.
//!
//! `let` and `FUNC_DEF` params are visited sequentially, extending [`Scope`]
//! as each binding/param is processed, so a later binding's value (for
//! `let`) or a later param's own matcher-default expression (for
//! `FUNC_DEF`) can resolve names bound earlier in the same construct — this
//! mirrors `pbexpr`'s actual evaluation-time scoping (see
//! `src/pbexpr/expr.rs`'s `Let`/`FuncDef` handling). `bind` differs: its
//! items don't see each other (only the body sees the bound names), which
//! is why `visit_bind` doesn't thread an extended scope into each item's
//! value the way `visit_let` does.

use std::convert::Infallible;
use std::ops::Range;
use std::rc::Rc;

use rowan::TextRange;
use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend,
};

use crate::pblang::syntaxtree::{SyntaxNode, SyntaxToken};
use crate::pblang::visit::{
    AssignKey, AttrSelector, LangVisitor, MapKind, ObjectField, StringPart, UnvisitedExpr,
    UnvisitedMatcher, visit_all, walk_expr,
};

use super::convert::LineIndex;
use super::document::OpenDocument;

pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![SemanticTokenType::VARIABLE, SemanticTokenType::PARAMETER],
        token_modifiers: vec![SemanticTokenModifier::DECLARATION],
    }
}

const TOKEN_TYPE_VARIABLE: u32 = 0;
const TOKEN_TYPE_PARAMETER: u32 = 1;
const MODIFIER_DECLARATION: u32 = 1 << 0;

/// Where a tracked identifier was bound. Anything not tracked by [`Scope`]
/// (object-literal keys, a matcher-object field's rename key, unresolved
/// references) has no `VarKind` and renders as a plain variable, exactly
/// as before this distinction existed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum VarKind {
    LetBound,
    FuncArg,
}

/// A cons-list lexical environment: extending it (`bind_one`) never
/// touches the existing frames, just links a new one in front via `Rc`, so
/// repeatedly extending it down a sequence of bindings stays cheap.
#[derive(Clone, Default)]
struct Scope(Option<Rc<ScopeFrame>>);

struct ScopeFrame {
    name: String,
    kind: VarKind,
    parent: Scope,
}

impl Scope {
    fn bind_one(&self, name: &str, kind: VarKind) -> Scope {
        Scope(Some(Rc::new(ScopeFrame {
            name: name.to_string(),
            kind,
            parent: self.clone(),
        })))
    }

    fn lookup(&self, name: &str) -> Option<VarKind> {
        let mut frame = &self.0;
        while let Some(f) = frame {
            if f.name == name {
                return Some(f.kind);
            }
            frame = &f.parent.0;
        }
        None
    }
}

/// One classified identifier: its span, whether it's a binding target
/// (`true`) or a variable reference (`false`), and — only for identifiers
/// [`Scope`] actually tracks — which kind of variable it is.
type Entry = (TextRange, bool, Option<VarKind>);

fn concat(mut a: Vec<Entry>, b: Vec<Entry>) -> Vec<Entry> {
    a.extend(b);
    a
}

struct SemanticVisitor;

impl LangVisitor for SemanticVisitor {
    type Expr = Vec<Entry>;
    /// Entries this matcher subtree can already fully resolve (from
    /// default expressions, and rename keys — see `visit_matcher_object`),
    /// plus the names this matcher subtree itself binds. A matcher can't
    /// classify its own bound names as `LetBound` or `FuncArg` — that's a
    /// property of where it's attached, not of the pattern itself — so it
    /// reports them upward for `visit_let`/`visit_func_def` to classify and
    /// fold into `Scope`.
    type Matcher = (Vec<Entry>, Vec<(TextRange, String)>);
    type Error = Infallible;
    type Down = Scope;

    fn visit_let(
        &mut self,
        _range: TextRange,
        down: &Scope,
        bindings: Vec<(TextRange, UnvisitedMatcher, UnvisitedExpr)>,
        body: UnvisitedExpr,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = Vec::new();
        let mut scope = down.clone();
        for (_, matcher, value) in bindings {
            // Sequential: this binding's value sees only prior bindings.
            out.extend(value.visit(self, &scope)?);
            let (entries, names) = matcher.visit(self, &scope)?;
            out.extend(entries);
            for (range, name) in names {
                out.push((range, true, Some(VarKind::LetBound)));
                scope = scope.bind_one(&name, VarKind::LetBound);
            }
        }
        out.extend(body.visit(self, &scope)?);
        Ok(out)
    }

    fn visit_bind(
        &mut self,
        _range: TextRange,
        down: &Scope,
        items: Vec<(TextRange, AssignKey, UnvisitedExpr)>,
        body: UnvisitedExpr,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = Vec::new();
        let mut scope = down.clone();
        for (_, key, value) in items {
            // Unlike `let`, bind items don't see each other — visited
            // against the unmodified incoming scope, not the growing one.
            out.extend(value.visit(self, down)?);
            if let AssignKey::Ident(token) = key {
                out.push((token.text_range(), true, Some(VarKind::LetBound)));
                scope = scope.bind_one(token.text(), VarKind::LetBound);
            }
        }
        out.extend(body.visit(self, &scope)?);
        Ok(out)
    }

    fn visit_func_def(
        &mut self,
        _range: TextRange,
        down: &Scope,
        params: Vec<UnvisitedMatcher>,
        body: UnvisitedExpr,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = Vec::new();
        let mut scope = down.clone();
        for param in params {
            // Sequential: a later param's own default expressions can see
            // an earlier param's bound names.
            let (entries, names) = param.visit(self, &scope)?;
            out.extend(entries);
            for (range, name) in names {
                out.push((range, true, Some(VarKind::FuncArg)));
                scope = scope.bind_one(&name, VarKind::FuncArg);
            }
        }
        out.extend(body.visit(self, &scope)?);
        Ok(out)
    }

    fn visit_binary(
        &mut self,
        _range: TextRange,
        down: &Scope,
        _op: SyntaxToken,
        lhs: UnvisitedExpr,
        rhs: UnvisitedExpr,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(concat(lhs.visit(self, down)?, rhs.visit(self, down)?))
    }

    fn visit_unary(
        &mut self,
        _range: TextRange,
        down: &Scope,
        _op: SyntaxToken,
        operand: UnvisitedExpr,
    ) -> Result<Vec<Entry>, Infallible> {
        operand.visit(self, down)
    }

    fn visit_func_call(
        &mut self,
        _range: TextRange,
        down: &Scope,
        func: UnvisitedExpr,
        arg: UnvisitedExpr,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(concat(func.visit(self, down)?, arg.visit(self, down)?))
    }

    fn visit_attr_sel(
        &mut self,
        _range: TextRange,
        down: &Scope,
        base: UnvisitedExpr,
        attr: AttrSelector<UnvisitedExpr>,
    ) -> Result<Vec<Entry>, Infallible> {
        let base = base.visit(self, down)?;
        let attr = match attr {
            AttrSelector::Dynamic(entries) => entries.visit(self, down)?,
            AttrSelector::Static(_) => Vec::new(),
        };
        Ok(concat(base, attr))
    }

    fn visit_fold(
        &mut self,
        _range: TextRange,
        down: &Scope,
        func: UnvisitedExpr,
        init: Option<UnvisitedExpr>,
        input: UnvisitedExpr,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = func.visit(self, down)?;
        if let Some(init) = init {
            out.extend(init.visit(self, down)?);
        }
        out.extend(input.visit(self, down)?);
        Ok(out)
    }

    fn visit_map(
        &mut self,
        _range: TextRange,
        down: &Scope,
        _kind: MapKind,
        func: UnvisitedExpr,
        input: UnvisitedExpr,
        filter: Option<UnvisitedExpr>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = func.visit(self, down)?;
        out.extend(input.visit(self, down)?);
        if let Some(filter) = filter {
            out.extend(filter.visit(self, down)?);
        }
        Ok(out)
    }

    fn visit_switch(
        &mut self,
        _range: TextRange,
        down: &Scope,
        input: UnvisitedExpr,
        cases: Vec<(UnvisitedExpr, UnvisitedExpr)>,
        default: Option<UnvisitedExpr>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = input.visit(self, down)?;
        for (pattern, result) in cases {
            out.extend(pattern.visit(self, down)?);
            out.extend(result.visit(self, down)?);
        }
        if let Some(default) = default {
            out.extend(default.visit(self, down)?);
        }
        Ok(out)
    }

    fn visit_object(
        &mut self,
        _range: TextRange,
        down: &Scope,
        items: Vec<(TextRange, AssignKey, UnvisitedExpr)>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = Vec::new();
        for (_, key, value) in items {
            if let AssignKey::Ident(token) = key {
                out.push((token.text_range(), true, None));
            }
            out.extend(value.visit(self, down)?);
        }
        Ok(out)
    }

    fn visit_list(
        &mut self,
        _range: TextRange,
        down: &Scope,
        items: Vec<UnvisitedExpr>,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(visit_all(items, self, down)?
            .into_iter()
            .flatten()
            .collect())
    }

    fn visit_tuple(
        &mut self,
        _range: TextRange,
        down: &Scope,
        items: Vec<UnvisitedExpr>,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(visit_all(items, self, down)?
            .into_iter()
            .flatten()
            .collect())
    }

    fn visit_literal(
        &mut self,
        _range: TextRange,
        _down: &Scope,
        _token: SyntaxToken,
    ) -> Result<Vec<Entry>, Infallible> {
        Ok(Vec::new())
    }

    fn visit_string(
        &mut self,
        _range: TextRange,
        down: &Scope,
        parts: Vec<StringPart<UnvisitedExpr>>,
    ) -> Result<Vec<Entry>, Infallible> {
        let mut out = Vec::new();
        for part in parts {
            if let StringPart::Embed(entries) = part {
                out.extend(entries.visit(self, down)?);
            }
        }
        Ok(out)
    }

    fn visit_var(
        &mut self,
        _range: TextRange,
        down: &Scope,
        name: SyntaxToken,
    ) -> Result<Vec<Entry>, Infallible> {
        let kind = down.lookup(name.text());
        Ok(vec![(name.text_range(), false, kind)])
    }

    fn visit_matcher_ident(
        &mut self,
        range: TextRange,
        _down: &Scope,
        name: SyntaxToken,
    ) -> Result<(Vec<Entry>, Vec<(TextRange, String)>), Infallible> {
        Ok((Vec::new(), vec![(range, name.text().to_string())]))
    }

    fn visit_matcher_wildcard(
        &mut self,
        _range: TextRange,
        _down: &Scope,
    ) -> Result<(Vec<Entry>, Vec<(TextRange, String)>), Infallible> {
        Ok((Vec::new(), Vec::new()))
    }

    fn visit_matcher_alias(
        &mut self,
        _range: TextRange,
        down: &Scope,
        inner: UnvisitedMatcher,
        name: SyntaxToken,
    ) -> Result<(Vec<Entry>, Vec<(TextRange, String)>), Infallible> {
        let (entries, mut names) = inner.visit(self, down)?;
        names.push((name.text_range(), name.text().to_string()));
        Ok((entries, names))
    }

    fn visit_matcher_tuple(
        &mut self,
        _range: TextRange,
        down: &Scope,
        items: Vec<UnvisitedMatcher>,
    ) -> Result<(Vec<Entry>, Vec<(TextRange, String)>), Infallible> {
        let mut all_entries = Vec::new();
        let mut all_names = Vec::new();
        for item in items {
            let (entries, names) = item.visit(self, down)?;
            all_entries.extend(entries);
            all_names.extend(names);
        }
        Ok((all_entries, all_names))
    }

    fn visit_matcher_object(
        &mut self,
        _range: TextRange,
        down: &Scope,
        _exhaustive: bool,
        fields: Vec<ObjectField<UnvisitedMatcher, UnvisitedExpr>>,
    ) -> Result<(Vec<Entry>, Vec<(TextRange, String)>), Infallible> {
        let mut all_entries = Vec::new();
        let mut all_names = Vec::new();
        for field in fields {
            match field.matcher {
                // Renamed form (`{a = pattern}`): `key` names a property of
                // the source object, not a new variable — the pattern's own
                // names are what gets bound.
                Some(matcher) => {
                    all_entries.push((field.key.text_range(), true, None));
                    let (entries, names) = matcher.visit(self, down)?;
                    all_entries.extend(entries);
                    all_names.extend(names);
                }
                // Shorthand form (`{foo}`): `key`'s text is itself the
                // bound variable's name.
                None => {
                    all_names.push((field.key.text_range(), field.key.text().to_string()));
                }
            }
            if let Some(default) = field.default {
                // Sees the scope outside this matcher, never a sibling
                // field bound by it.
                all_entries.extend(default.visit(self, down)?);
            }
        }
        Ok((all_entries, all_names))
    }
}

/// Walks the tree, classifying every declaration/reference identifier, and
/// returns them in document order. Pure tree-shape lookup, kept separate
/// from wire-format encoding so it's testable on its own.
fn classify_tokens(node: &SyntaxNode) -> Vec<(Range<usize>, bool, Option<VarKind>)> {
    let mut visitor = SemanticVisitor;
    let mut entries = walk_expr(node, &mut visitor, &Scope::default()).unwrap();
    entries.sort_by_key(|(range, _, _)| range.start());
    entries
        .into_iter()
        .map(|(range, is_declaration, kind)| {
            (
                usize::from(range.start())..usize::from(range.end()),
                is_declaration,
                kind,
            )
        })
        .collect()
}

/// Delta-encodes classified spans (already in document order) into the
/// wire format `textDocument/semanticTokens/full` expects.
fn encode_semantic_tokens(
    source: &str,
    line_index: &LineIndex,
    classified: &[(Range<usize>, bool, Option<VarKind>)],
) -> Vec<SemanticToken> {
    let mut tokens = Vec::with_capacity(classified.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
    for (span, is_declaration, kind) in classified {
        let start = line_index.position(source, span.start);
        let length = source[span.clone()].encode_utf16().count() as u32;
        let delta_line = start.line - prev_line;
        let delta_start = if delta_line == 0 {
            start.character - prev_start
        } else {
            start.character
        };
        let token_type = match kind {
            Some(VarKind::FuncArg) => TOKEN_TYPE_PARAMETER,
            Some(VarKind::LetBound) | None => TOKEN_TYPE_VARIABLE,
        };
        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
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

    #[test]
    fn func_arg_is_colored_as_a_parameter_at_both_sites() {
        let tokens = tokens_for("|x| x");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token_type, TOKEN_TYPE_PARAMETER); // param decl
        assert_eq!(tokens[0].token_modifiers_bitset, MODIFIER_DECLARATION);
        assert_eq!(tokens[1].token_type, TOKEN_TYPE_PARAMETER); // reference
        assert_eq!(tokens[1].token_modifiers_bitset, 0);
    }

    #[test]
    fn let_bound_and_func_arg_are_colored_differently_in_the_same_snippet() {
        let tokens = tokens_for("let a = 1; in |x| a + x");
        // "a" (let decl), "a" (ref), "x" (param decl), "x" (ref) — in that
        // walk order (let's body, `|x| a + x`, visited after `a`'s own
        // binding, and `a + x` visits `a` before `x`).
        assert_eq!(tokens.len(), 4);
        let by_type: Vec<u32> = tokens.iter().map(|t| t.token_type).collect();
        assert_eq!(
            by_type,
            vec![
                TOKEN_TYPE_VARIABLE,  // a decl
                TOKEN_TYPE_PARAMETER, // x decl
                TOKEN_TYPE_VARIABLE,  // a ref
                TOKEN_TYPE_PARAMETER, // x ref
            ]
        );
    }

    #[test]
    fn sequential_let_bindings_see_earlier_siblings() {
        let tokens = tokens_for("let a = 1; b = a; in b");
        // "a" decl, "b" decl, "a" ref (inside b's value), "b" ref (body).
        assert_eq!(tokens.len(), 4);
        // The reference to `a` (index 2) must have resolved, i.e. it's
        // colored as a tracked variable, not left as an untracked one —
        // both render as TOKEN_TYPE_VARIABLE either way, so check via the
        // underlying classification instead of the encoded token type.
        let node = pblang::parse("let a = 1; b = a; in b").tree.unwrap();
        let classified = classify_tokens(&node);
        let a_ref = classified
            .iter()
            .find(|(_, is_decl, _)| !is_decl)
            .expect("a reference exists");
        assert_eq!(a_ref.2, Some(VarKind::LetBound));
    }

    #[test]
    fn bind_item_value_does_not_see_a_sibling_bind_item() {
        let source = "bind a = 1; b = a; in b";
        let node = pblang::parse(source).tree.unwrap();
        let classified = classify_tokens(&node);
        // The reference to `a` inside `b`'s value must NOT resolve, since
        // bind items don't see each other.
        let a_ref = classified
            .iter()
            .find(|(_, is_decl, _)| !is_decl)
            .expect("a reference exists");
        assert_eq!(a_ref.2, None);
    }
}
