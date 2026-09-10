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
    AssignKey, AttrSelector, LangVisitor, MapKind, ObjectField, StringPart, walk_expr,
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
    /// matcher pattern.
    type Matcher = (String, TextRange);
    type Error = Infallible;

    fn visit_let(
        &mut self,
        _range: TextRange,
        bindings: Vec<(TextRange, (String, TextRange), Vec<DocumentSymbol>)>,
        body: Vec<DocumentSymbol>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        let mut symbols = Vec::new();
        for (stmt_range, (name, name_range), nested) in bindings {
            symbols.push(self.binding_symbol(stmt_range, name, name_range, nested));
        }
        symbols.extend(body);
        Ok(symbols)
    }

    fn visit_bind(
        &mut self,
        _range: TextRange,
        items: Vec<(TextRange, AssignKey, Vec<DocumentSymbol>)>,
        body: Vec<DocumentSymbol>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        let mut symbols = Vec::new();
        for (stmt_range, key, nested) in items {
            let (name, name_range) = self.assign_key_text_and_range(&key);
            symbols.push(self.binding_symbol(stmt_range, name, name_range, nested));
        }
        symbols.extend(body);
        Ok(symbols)
    }

    fn visit_func_def(
        &mut self,
        _range: TextRange,
        _params: Vec<(String, TextRange)>,
        body: Vec<DocumentSymbol>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(body)
    }

    fn visit_binary(
        &mut self,
        _range: TextRange,
        _op: SyntaxToken,
        lhs: Vec<DocumentSymbol>,
        rhs: Vec<DocumentSymbol>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(concat(lhs, rhs))
    }

    fn visit_unary(
        &mut self,
        _range: TextRange,
        _op: SyntaxToken,
        operand: Vec<DocumentSymbol>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(operand)
    }

    fn visit_func_call(
        &mut self,
        _range: TextRange,
        func: Vec<DocumentSymbol>,
        arg: Vec<DocumentSymbol>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(concat(func, arg))
    }

    fn visit_attr_sel(
        &mut self,
        _range: TextRange,
        base: Vec<DocumentSymbol>,
        attr: AttrSelector<Vec<DocumentSymbol>>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        let attr = match attr {
            AttrSelector::Dynamic(nested) => nested,
            AttrSelector::Static(_) => Vec::new(),
        };
        Ok(concat(base, attr))
    }

    fn visit_fold(
        &mut self,
        _range: TextRange,
        func: Vec<DocumentSymbol>,
        init: Option<Vec<DocumentSymbol>>,
        input: Vec<DocumentSymbol>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        let mut out = func;
        out.extend(init.into_iter().flatten());
        out.extend(input);
        Ok(out)
    }

    fn visit_map(
        &mut self,
        _range: TextRange,
        _kind: MapKind,
        func: Vec<DocumentSymbol>,
        input: Vec<DocumentSymbol>,
        filter: Option<Vec<DocumentSymbol>>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        let mut out = func;
        out.extend(input);
        out.extend(filter.into_iter().flatten());
        Ok(out)
    }

    fn visit_switch(
        &mut self,
        _range: TextRange,
        input: Vec<DocumentSymbol>,
        cases: Vec<(Vec<DocumentSymbol>, Vec<DocumentSymbol>)>,
        default: Option<Vec<DocumentSymbol>>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
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
        items: Vec<(TextRange, AssignKey, Vec<DocumentSymbol>)>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        let mut symbols = Vec::new();
        for (stmt_range, key, nested) in items {
            let (name, name_range) = self.assign_key_text_and_range(&key);
            symbols.push(self.binding_symbol(stmt_range, name, name_range, nested));
        }
        Ok(symbols)
    }

    fn visit_list(
        &mut self,
        _range: TextRange,
        items: Vec<Vec<DocumentSymbol>>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(items.into_iter().flatten().collect())
    }

    fn visit_tuple(
        &mut self,
        _range: TextRange,
        items: Vec<Vec<DocumentSymbol>>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(items.into_iter().flatten().collect())
    }

    fn visit_literal(
        &mut self,
        _range: TextRange,
        _token: SyntaxToken,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(Vec::new())
    }

    fn visit_string(
        &mut self,
        _range: TextRange,
        parts: Vec<StringPart<Vec<DocumentSymbol>>>,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(parts
            .into_iter()
            .filter_map(|part| match part {
                StringPart::Embed(nested) => Some(nested),
                StringPart::Chunk(_) => None,
            })
            .flatten()
            .collect())
    }

    fn visit_var(
        &mut self,
        _range: TextRange,
        _name: SyntaxToken,
    ) -> Result<Vec<DocumentSymbol>, Infallible> {
        Ok(Vec::new())
    }

    fn visit_matcher_ident(
        &mut self,
        range: TextRange,
        _name: SyntaxToken,
    ) -> Result<(String, TextRange), Infallible> {
        Ok((self.text_at(range), range))
    }

    fn visit_matcher_wildcard(
        &mut self,
        range: TextRange,
    ) -> Result<(String, TextRange), Infallible> {
        Ok((self.text_at(range), range))
    }

    fn visit_matcher_alias(
        &mut self,
        range: TextRange,
        _inner: (String, TextRange),
        _name: SyntaxToken,
    ) -> Result<(String, TextRange), Infallible> {
        Ok((self.text_at(range), range))
    }

    fn visit_matcher_tuple(
        &mut self,
        range: TextRange,
        _items: Vec<(String, TextRange)>,
    ) -> Result<(String, TextRange), Infallible> {
        Ok((self.text_at(range), range))
    }

    fn visit_matcher_object(
        &mut self,
        range: TextRange,
        _exhaustive: bool,
        _fields: Vec<ObjectField<(String, TextRange), Vec<DocumentSymbol>>>,
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
        Some(walk_expr(&node, &mut visitor).unwrap())
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
