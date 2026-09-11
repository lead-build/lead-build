//! `textDocument/documentSymbol`: lists binding targets (`let`/`bind`
//! entries, object assignments) for the outline/breadcrumbs view, nested by
//! tree containment — e.g. an assignment's own value expression is walked
//! for further bindings, which become its children. This is pure tree-shape
//! containment, not scope resolution: no attempt is made to decide what's
//! actually *visible* where, just what's nested under what.
//!
//! Built on [`crate::pblang::visit::LangVisitor`]: `Self::Expr` is the
//! `Vec<DocumentSymbol>` found so far in a subtree — most node kinds just
//! flatten their children's fragments together, while `LET_EXPR`,
//! `BIND_EXPR`, and `OBJECT_EXPR` each turn their own bindings into one
//! `DocumentSymbol`, nesting whatever was found in that binding's value.

use std::convert::Infallible;

use rowan::TextRange;
use tower_lsp::lsp_types::{DocumentSymbol, Range as LspRange, SymbolKind};

use crate::pblang::syntaxtree::SyntaxToken;
use crate::pblang::visit::{
    AssignKey, AttrSelector, LangVisitor, MapKind, ObjectField, StringPart, UnvisitedExpr,
    UnvisitedMatcher, visit_all, walk_expr,
};

use super::convert::LineIndex;
use super::document::OpenDocument;

#[allow(deprecated)]
fn make_symbol(
    name: String,
    range: LspRange,
    selection_range: LspRange,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail: None,
        kind: SymbolKind::VARIABLE,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }
}

fn concat(mut a: Vec<DocumentSymbol>, b: Vec<DocumentSymbol>) -> Vec<DocumentSymbol> {
    a.extend(b);
    a
}

struct SymbolVisitor<'a> {
    line_index: &'a LineIndex,
    source: &'a str,
}

impl SymbolVisitor<'_> {
    fn text_at(&self, range: TextRange) -> String {
        self.source[usize::from(range.start())..usize::from(range.end())].to_string()
    }

    fn lsp_range(&self, range: TextRange) -> LspRange {
        self.line_index.range(self.source, range.into())
    }

    fn binding_symbol(
        &self,
        stmt_range: TextRange,
        name: String,
        name_range: TextRange,
        nested: Vec<DocumentSymbol>,
    ) -> DocumentSymbol {
        make_symbol(
            name,
            self.lsp_range(stmt_range),
            self.lsp_range(name_range),
            nested,
        )
    }

    /// An `ASSIGNMENT`'s key, as `binding_symbol` needs it: the key is
    /// either a bare `IDENT` token, or a `STRING_LIT` node — its own
    /// rendered text either way, with its own range as the selection range.
    fn assign_key_text_and_range(&self, key: &AssignKey) -> (String, TextRange) {
        match key {
            AssignKey::Ident(token) => (token.text().to_string(), token.text_range()),
            AssignKey::StringLit(node) => (node.text().to_string(), node.text_range()),
        }
    }
}

impl LangVisitor for SymbolVisitor<'_> {
    type Expr = Vec<DocumentSymbol>;
    /// A matcher's own rendered text (its whole node's source slice — works
    /// uniformly for a plain `let x = ..` ident and a compound pattern like
    /// `let (a, b) = ..`) plus its own range, for use as a binding's name
    /// and selection range. Nothing is recursed into a matcher's internals
    /// for symbols — matches today's behavior of only ever surfacing
    /// `ASSIGNMENT`/`LET_BINDING` as symbols, never anything nested inside a
    /// matcher pattern. Since that text/range comes straight off an
    /// `UnvisitedMatcher`'s own span, every `visit_matcher_*` method below
    /// this impl is simply never called — no matcher subtree is walked at
    /// all.
    type Matcher = (String, TextRange);
    type Error = Infallible;
    type Down = ();

    fn visit_let(
        &mut self,
        _range: TextRange,
        down: &(),
        bindings: Vec<(TextRange, UnvisitedMatcher, UnvisitedExpr)>,
        body: UnvisitedExpr,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        let mut symbols = Vec::new();
        for (stmt_range, matcher, value) in bindings {
            let name_range = matcher.range();
            let name = self.text_at(name_range);
            let nested = value.visit(self, down)?;
            symbols.push(self.binding_symbol(stmt_range, name, name_range, nested));
        }
        symbols.extend(body.visit(self, down)?);
        Ok(symbols)
    }

    fn visit_bind(
        &mut self,
        _range: TextRange,
        down: &(),
        items: Vec<(TextRange, AssignKey, UnvisitedExpr)>,
        body: UnvisitedExpr,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        let mut symbols = Vec::new();
        for (stmt_range, key, value) in items {
            let (name, name_range) = self.assign_key_text_and_range(&key);
            let nested = value.visit(self, down)?;
            symbols.push(self.binding_symbol(stmt_range, name, name_range, nested));
        }
        symbols.extend(body.visit(self, down)?);
        Ok(symbols)
    }

    fn visit_func_def(
        &mut self,
        _range: TextRange,
        down: &(),
        _params: Vec<UnvisitedMatcher>,
        body: UnvisitedExpr,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        body.visit(self, down)
    }

    fn visit_binary(
        &mut self,
        _range: TextRange,
        down: &(),
        _op: SyntaxToken,
        lhs: UnvisitedExpr,
        rhs: UnvisitedExpr,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(concat(lhs.visit(self, down)?, rhs.visit(self, down)?))
    }

    fn visit_unary(
        &mut self,
        _range: TextRange,
        down: &(),
        _op: SyntaxToken,
        operand: UnvisitedExpr,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        operand.visit(self, down)
    }

    fn visit_func_call(
        &mut self,
        _range: TextRange,
        down: &(),
        func: UnvisitedExpr,
        arg: UnvisitedExpr,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(concat(func.visit(self, down)?, arg.visit(self, down)?))
    }

    fn visit_attr_sel(
        &mut self,
        _range: TextRange,
        down: &(),
        base: UnvisitedExpr,
        attr: AttrSelector<UnvisitedExpr>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        let base = base.visit(self, down)?;
        let attr = match attr {
            AttrSelector::Dynamic(nested) => nested.visit(self, down)?,
            AttrSelector::Static(_) => Vec::new(),
        };
        Ok(concat(base, attr))
    }

    fn visit_fold(
        &mut self,
        _range: TextRange,
        down: &(),
        func: UnvisitedExpr,
        init: Option<UnvisitedExpr>,
        input: UnvisitedExpr,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
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
        down: &(),
        _kind: MapKind,
        func: UnvisitedExpr,
        input: UnvisitedExpr,
        filter: Option<UnvisitedExpr>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
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
        down: &(),
        input: UnvisitedExpr,
        cases: Vec<(UnvisitedExpr, UnvisitedExpr)>,
        default: Option<UnvisitedExpr>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
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
        down: &(),
        items: Vec<(TextRange, AssignKey, UnvisitedExpr)>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        let mut symbols = Vec::new();
        for (stmt_range, key, value) in items {
            let (name, name_range) = self.assign_key_text_and_range(&key);
            let nested = value.visit(self, down)?;
            symbols.push(self.binding_symbol(stmt_range, name, name_range, nested));
        }
        Ok(symbols)
    }

    fn visit_list(
        &mut self,
        _range: TextRange,
        down: &(),
        items: Vec<UnvisitedExpr>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(visit_all(items, self, down)?
            .into_iter()
            .flatten()
            .collect())
    }

    fn visit_tuple(
        &mut self,
        _range: TextRange,
        down: &(),
        items: Vec<UnvisitedExpr>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(visit_all(items, self, down)?
            .into_iter()
            .flatten()
            .collect())
    }

    fn visit_literal(
        &mut self,
        _range: TextRange,
        _down: &(),
        _token: SyntaxToken,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(Vec::new())
    }

    fn visit_string(
        &mut self,
        _range: TextRange,
        down: &(),
        parts: Vec<StringPart<UnvisitedExpr>>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        let mut out = Vec::new();
        for part in parts {
            if let StringPart::Embed(nested) = part {
                out.extend(nested.visit(self, down)?);
            }
        }
        Ok(out)
    }

    fn visit_var(
        &mut self,
        _range: TextRange,
        _down: &(),
        _name: SyntaxToken,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(Vec::new())
    }

    fn visit_matcher_ident(
        &mut self,
        range: TextRange,
        _down: &(),
        _name: SyntaxToken,
    ) -> Result<(String, TextRange), Infallible> {
        Ok((self.text_at(range), range))
    }

    fn visit_matcher_wildcard(
        &mut self,
        range: TextRange,
        _down: &(),
    ) -> Result<(String, TextRange), Infallible> {
        Ok((self.text_at(range), range))
    }

    fn visit_matcher_alias(
        &mut self,
        range: TextRange,
        _down: &(),
        _inner: UnvisitedMatcher,
        _name: SyntaxToken,
    ) -> Result<(String, TextRange), Infallible> {
        Ok((self.text_at(range), range))
    }

    fn visit_matcher_tuple(
        &mut self,
        range: TextRange,
        _down: &(),
        _items: Vec<UnvisitedMatcher>,
    ) -> Result<(String, TextRange), Infallible> {
        Ok((self.text_at(range), range))
    }

    fn visit_matcher_object(
        &mut self,
        range: TextRange,
        _down: &(),
        _exhaustive: bool,
        _fields: Vec<ObjectField<UnvisitedMatcher, UnvisitedExpr>>,
    ) -> Result<(String, TextRange), Infallible> {
        Ok((self.text_at(range), range))
    }
}

impl OpenDocument {
    /// `None` when the document's latest parse failed — nothing to
    /// outline until it parses again.
    pub fn document_symbols(&self) -> Option<Vec<DocumentSymbol>> {
        let node = self.syntax_node()?;
        let mut visitor = SymbolVisitor {
            line_index: &self.line_index,
            source: &self.text,
        };
        Some(walk_expr(&node, &mut visitor, &()).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols_for(source: &str) -> Vec<DocumentSymbol> {
        OpenDocument::new(source.to_string())
            .document_symbols()
            .expect("valid source")
    }

    #[test]
    fn top_level_let_binding_is_a_symbol() {
        let symbols = symbols_for("let x = null; in x");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "x");
        assert_eq!(symbols[0].kind, SymbolKind::VARIABLE);
    }

    #[test]
    fn nested_object_assignment_becomes_a_child_symbol() {
        let symbols = symbols_for("bind y = { a = null; }; in y");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "y");
        let children = symbols[0].children.as_ref().expect("nested children");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "a");
    }

    #[test]
    fn expression_with_no_bindings_has_no_symbols() {
        assert_eq!(symbols_for("null"), Vec::new());
    }
}
