//! The `tower_lsp::LanguageServer` implementation for `pbls`.

use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, DocumentSymbolParams, DocumentSymbolResponse, FoldingRange,
    FoldingRangeParams, FoldingRangeProviderCapability, GotoDefinitionParams,
    GotoDefinitionResponse, InitializeParams, InitializeResult, InitializedParams, Location,
    MessageType, OneOf, SemanticTokens, SemanticTokensFullOptions, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
};
use tower_lsp::{Client, LanguageServer, async_trait};

use super::document::ClientDocuments;
use super::semantic_tokens::legend;

pub struct Backend {
    client: Client,
    documents: ClientDocuments,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: ClientDocuments::default(),
        }
    }

    async fn publish_diagnostics_for(&self, uri: Url) {
        let diagnostics = self
            .documents
            .get(&uri)
            .map(|doc| doc.diagnostics())
            .unwrap_or_default();
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
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
                definition_provider: Some(OneOf::Left(true)),
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
        self.documents
            .insert(uri.clone(), params.text_document.text);
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
        let data = self
            .documents
            .get(&params.text_document.uri)
            .and_then(|doc| doc.semantic_tokens());
        Ok(data.map(|data| {
            SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data,
            })
        }))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> RpcResult<Option<DocumentSymbolResponse>> {
        let symbols = self
            .documents
            .get(&params.text_document.uri)
            .and_then(|doc| doc.document_symbols());
        Ok(symbols.map(DocumentSymbolResponse::Nested))
    }

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> RpcResult<Option<Vec<FoldingRange>>> {
        Ok(self
            .documents
            .get(&params.text_document.uri)
            .and_then(|doc| doc.folding_ranges()))
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> RpcResult<Option<Vec<TextEdit>>> {
        Ok(self
            .documents
            .get(&params.text_document.uri)
            .and_then(|doc| doc.formatting_edits()))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> RpcResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let range = self
            .documents
            .get(&uri)
            .and_then(|doc| doc.goto_definition(position));
        Ok(range.map(|range| GotoDefinitionResponse::Scalar(Location { uri, range })))
    }
}
