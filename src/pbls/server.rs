//! The `tower_lsp::LanguageServer` implementation for `pbls`.

use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, DocumentSymbolParams, DocumentSymbolResponse, FoldingRange,
    FoldingRangeParams, FoldingRangeProviderCapability, InitializeParams, InitializeResult,
    InitializedParams, MessageType, OneOf, SemanticTokens, SemanticTokensFullOptions,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Url,
};
use tower_lsp::{async_trait, Client, LanguageServer};

use crate::pblang::syntaxtree::SyntaxNode;

use super::convert::LineIndex;
use super::diagnostics::diagnostics_for_source;
use super::document::Documents;
use super::document_symbols::document_symbols_for_source;
use super::folding_ranges::folding_ranges_for_source;
use super::formatting::formatting_edits_for_source;
use super::semantic_tokens::{legend, semantic_tokens_for_source};

pub struct Backend {
    client: Client,
    documents: Documents,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Documents::default(),
        }
    }

    async fn publish_diagnostics_for(&self, uri: Url) {
        let diagnostics = self
            .documents
            .with(&uri, |doc| diagnostics_for_source(&doc.text, &doc.line_index))
            .unwrap_or_default();
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    /// Runs `f` against a document's current source text, rebuilt syntax
    /// tree, and line index — `None` if the document isn't open, or its
    /// latest parse failed (nothing to show for these features until it
    /// parses again).
    fn with_tree<R>(&self, uri: &Url, f: impl FnOnce(&str, &SyntaxNode, &LineIndex) -> R) -> Option<R> {
        self.documents
            .with(uri, |doc| doc.syntax_node().map(|node| f(&doc.text, &node, &doc.line_index)))
            .flatten()
    }
}

#[async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> RpcResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: legend(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..SemanticTokensOptions::default()
                        },
                    ),
                ),
                document_symbol_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "pbls initialized")
            .await;
    }

    async fn shutdown(&self) -> RpcResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.insert(uri.clone(), params.text_document.text);
        self.publish_diagnostics_for(uri).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // We advertise `TextDocumentSyncKind::FULL`, so there's always
        // exactly one change with the whole new document text.
        let text = params.content_changes.remove(0).text;
        self.documents.insert(uri.clone(), text);
        self.publish_diagnostics_for(uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> RpcResult<Option<SemanticTokensResult>> {
        let data = self.with_tree(&params.text_document.uri, semantic_tokens_for_source);
        Ok(data.map(|data| SemanticTokensResult::Tokens(SemanticTokens { result_id: None, data })))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> RpcResult<Option<DocumentSymbolResponse>> {
        let symbols = self.with_tree(&params.text_document.uri, |source, node, line_index| {
            document_symbols_for_source(node, line_index, source)
        });
        Ok(symbols.map(DocumentSymbolResponse::Nested))
    }

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> RpcResult<Option<Vec<FoldingRange>>> {
        Ok(self.with_tree(&params.text_document.uri, |source, node, line_index| {
            folding_ranges_for_source(node, line_index, source)
        }))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> RpcResult<Option<Vec<TextEdit>>> {
        Ok(self.with_tree(&params.text_document.uri, formatting_edits_for_source))
    }
}
