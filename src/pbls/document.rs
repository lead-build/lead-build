//! In-memory store of currently open documents. Each document is reparsed
//! in full on every change — no incremental parsing, no cross-file context.
//!
//! `OpenDocument`'s core lifecycle (`new`/`syntax_node`) lives here. Its
//! LSP-facing methods (`diagnostics`, `semantic_tokens`, `document_symbols`,
//! `folding_ranges`, `formatting_edits`) are implemented in sibling modules,
//! next to the feature logic and tests they belong with, via separate `impl
//! OpenDocument` blocks — one per file, not all crammed in here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rowan::GreenNode;
use tower_lsp::lsp_types::Url;

use crate::pblang::{self, syntaxtree::SyntaxNode};

use super::convert::LineIndex;

/// The parsed state of one open document.
///
/// This stores the *green* tree, not rowan's red `SyntaxNode`: the red tree
/// (`rowan::cursor::SyntaxNode`) is built on raw, non-atomically-refcounted
/// pointers and so isn't `Send`/`Sync`, which `ClientDocuments` needs to be
/// since it's shared across `tower-lsp`'s async handlers. `GreenNode` is
/// `Arc`-based and cheap to clone, so callers rebuild a red `SyntaxNode` from
/// it on demand via [`OpenDocument::syntax_node`].
pub struct OpenDocument {
    pub text: String,
    pub line_index: LineIndex,
    /// `None` when the latest parse of `text` failed.
    green: Option<GreenNode>,
}

impl OpenDocument {
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

/// All currently open documents for one client, keyed by their LSP `Url`.
#[derive(Default)]
pub struct ClientDocuments(Mutex<HashMap<Url, Arc<OpenDocument>>>);

impl ClientDocuments {
    pub fn insert(&self, uri: Url, text: String) {
        self.0
            .lock()
            .unwrap()
            .insert(uri, Arc::new(OpenDocument::new(text)));
    }

    pub fn remove(&self, uri: &Url) {
        self.0.lock().unwrap().remove(uri);
    }

    /// The open document for `uri`, if any. Cloning the returned `Arc` is
    /// cheap (an atomic refcount bump, not a copy of the document's text),
    /// so the map's lock is only held long enough to look the entry up —
    /// callers use the document after it's released.
    pub fn get(&self, uri: &Url) -> Option<Arc<OpenDocument>> {
        self.0.lock().unwrap().get(uri).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_text_parses_to_a_tree() {
        let state = OpenDocument::new("null".to_string());
        assert!(state.syntax_node().is_some());
    }

    #[test]
    fn invalid_text_has_no_tree() {
        let state = OpenDocument::new("let x = in x".to_string());
        assert!(state.syntax_node().is_none());
    }
}
