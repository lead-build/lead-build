//! In-memory store of currently open documents. Each document is reparsed
//! in full on every change — no incremental parsing, no cross-file context.

use std::collections::HashMap;
use std::sync::Mutex;

use rowan::GreenNode;
use tower_lsp::lsp_types::Url;

use crate::pblang::{self, syntaxtree::SyntaxNode};

use super::convert::LineIndex;

/// The parsed state of one open document.
///
/// This stores the *green* tree, not rowan's red `SyntaxNode`: the red tree
/// (`rowan::cursor::SyntaxNode`) is built on raw, non-atomically-refcounted
/// pointers and so isn't `Send`/`Sync`, which `Documents` needs to be since
/// it's shared across `tower-lsp`'s async handlers. `GreenNode` is
/// `Arc`-based and cheap to clone, so callers rebuild a red `SyntaxNode` from
/// it on demand via [`DocumentState::syntax_node`].
pub struct DocumentState {
    pub text: String,
    pub line_index: LineIndex,
    /// `None` when the latest parse of `text` failed.
    pub green: Option<GreenNode>,
}

impl DocumentState {
    pub fn new(text: String) -> Self {
        let line_index = LineIndex::new(&text);
        let green = pblang::parse(&text).ok().map(|node| node.green().to_owned());
        Self {
            text,
            line_index,
            green,
        }
    }

    pub fn syntax_node(&self) -> Option<SyntaxNode> {
        self.green.clone().map(SyntaxNode::new_root)
    }
}

/// All currently open documents, keyed by their LSP `Url`.
#[derive(Default)]
pub struct Documents(Mutex<HashMap<Url, DocumentState>>);

impl Documents {
    pub fn insert(&self, uri: Url, text: String) {
        self.0.lock().unwrap().insert(uri, DocumentState::new(text));
    }

    pub fn remove(&self, uri: &Url) {
        self.0.lock().unwrap().remove(uri);
    }

    pub fn with<R>(&self, uri: &Url, f: impl FnOnce(&DocumentState) -> R) -> Option<R> {
        self.0.lock().unwrap().get(uri).map(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_text_parses_to_a_tree() {
        let state = DocumentState::new("null".to_string());
        assert!(state.syntax_node().is_some());
    }

    #[test]
    fn invalid_text_has_no_tree() {
        let state = DocumentState::new("let x = in x".to_string());
        assert!(state.syntax_node().is_none());
    }
}
