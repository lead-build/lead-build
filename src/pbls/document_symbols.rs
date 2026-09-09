//! `textDocument/documentSymbol`: lists binding targets (`let`/`bind`
//! entries, object assignments) for the outline/breadcrumbs view, nested by
//! tree containment — e.g. an assignment's own value expression is walked
//! for further bindings, which become its children. This is pure tree-shape
//! containment, not scope resolution: no attempt is made to decide what's
//! actually *visible* where, just what's nested under what.

use tower_lsp::lsp_types::{DocumentSymbol, SymbolKind};

use crate::pblang::syntaxtree::{SyntaxElement, SyntaxKind, SyntaxNode};

use super::convert::LineIndex;
use super::document::OpenDocument;

/// `ASSIGNMENT` and `LET_BINDING` both have exactly four non-trivia children,
/// in a fixed order — see `grammar.lalrpop`'s `BindSetStmt`/`AssignStmt`
/// (`vec![key, eq, value, semi]`) and `LetSetStmt` (`vec![matcher, eq, value,
/// semi]`) productions.
fn non_trivia_children(node: &SyntaxNode) -> Vec<SyntaxElement> {
    node.children_with_tokens()
        .filter(|e| e.kind() != SyntaxKind::TRIVIA)
        .collect()
}

fn binding_name(name_element: &SyntaxElement) -> String {
    match name_element {
        SyntaxElement::Token(token) => token.text().to_string(),
        SyntaxElement::Node(node) => node.text().to_string(),
    }
}

#[allow(deprecated)]
fn make_symbol(
    name: String,
    node: &SyntaxNode,
    name_element: &SyntaxElement,
    line_index: &LineIndex,
    source: &str,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail: None,
        kind: SymbolKind::VARIABLE,
        tags: None,
        deprecated: None,
        range: line_index.range(source, node.text_range().into()),
        selection_range: line_index.range(source, name_element.text_range().into()),
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }
}

/// Walks `node`'s children looking for `ASSIGNMENT`/`LET_BINDING` nodes;
/// anything else is descended into (without becoming a symbol itself) since
/// bindings can be nested arbitrarily deep under `FUNC_DEF`/`LET_EXPR`/
/// `BIND_EXPR`/`OBJECT_EXPR`/etc.
fn collect_symbols(node: &SyntaxNode, line_index: &LineIndex, source: &str) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    for child in node.children() {
        match child.kind() {
            SyntaxKind::ASSIGNMENT | SyntaxKind::LET_BINDING => {
                let elements = non_trivia_children(&child);
                let (Some(name_element), Some(value)) =
                    (elements.first(), elements.get(2).and_then(|e| e.as_node()))
                else {
                    continue;
                };
                let name = binding_name(name_element);
                let nested = collect_symbols(value, line_index, source);
                symbols.push(make_symbol(
                    name,
                    &child,
                    name_element,
                    line_index,
                    source,
                    nested,
                ));
            }
            _ => symbols.extend(collect_symbols(&child, line_index, source)),
        }
    }
    symbols
}

impl OpenDocument {
    /// `None` when the document's latest parse failed — nothing to
    /// outline until it parses again.
    pub fn document_symbols(&self) -> Option<Vec<DocumentSymbol>> {
        let node = self.syntax_node()?;
        Some(collect_symbols(&node, &self.line_index, &self.text))
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
