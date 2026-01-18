use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::ast::*;
use crate::parser::{parse_file, check_semantics};

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub documents: Arc<RwLock<HashMap<Url, String>>>,
}

pub fn build_document_symbols(file: &AmaroFile) -> Vec<DocumentSymbol> {
    file.blocks.iter().map(|block| {
        let kind = match block.kind.as_str() {
            "GateRealization" | "Transition" | "Architecture" | "Arch" => SymbolKind::CLASS,
            "Step" => SymbolKind::FUNCTION,
            "RouteInfo" | "TransitionInfo" | "ArchInfo" | "StateInfo" => SymbolKind::MODULE,
            _ => SymbolKind::OBJECT,
        };

        #[allow(deprecated)]
        let children: Vec<DocumentSymbol> = block.fields.iter().map(|field| {
            DocumentSymbol {
                name: field.key.clone(),
                detail: Some(field.value.chars().take(30).collect()), // Preview value
                kind: SymbolKind::FIELD, // Shows as a little blue tag
                tags: None,
                deprecated: None,
                range: field.key_range,
                selection_range: field.key_range,
                children: None,
            }
        }).collect();

        #[allow(deprecated)]
        DocumentSymbol {
            name: block.kind.clone(),
            detail: None,
            kind,
            tags: None,
            deprecated: None,
            range: block.range,
            selection_range: block.range,
            children: if children.is_empty() { None } else { Some(children) },
        }
    }).collect()
}


impl Backend {
    pub fn new(client: Client) -> Self {
        Backend {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // Validating Document
    pub async fn validate_document(&self, uri: Url, text: String) {
        let mut diagnostics = Vec::new();

        // Syntactic Analysis
        match parse_file(&text) {
            Ok(file) => {
                // Semantic Checks
                let ast_debug = format!("{:#?}", file);
                self.client.log_message(MessageType::INFO, format!("Parsed AST:\n{}", ast_debug)).await;
                
                let mut semantic_errors = check_semantics(&file);
                diagnostics.append(&mut semantic_errors);
            }
            Err(_) => {
                diagnostics.push(Diagnostic {
                    range: Range::default(),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: "Fatal Syntax Error: Parsing aborted.".to_string(),
                    ..Default::default()
                });
            }
        }

        self.client.publish_diagnostics(uri, diagnostics, Some(1)).await;
    }
}


#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                
                // Phase 1: Enable Document Symbols
                document_symbol_provider: Some(OneOf::Left(true)),

                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Amaro LSP initialized!")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();

        self.documents.write().await.insert(uri.clone(), text.clone());

        self.client
            .log_message(MessageType::INFO, "Amaro file opened!")
            .await;
        self.validate_document(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        if let Some(change) = params.content_changes.into_iter().next() {
            let text = change.text.clone();
            self.documents.write().await.insert(uri.clone(), text.clone());

            self.validate_document(uri, text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.write().await.remove(&params.text_document.uri);
        self.client.publish_diagnostics(
            params.text_document.uri,
            vec![],
            Some(1)
        ).await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // Phase 1: Document Symbols Implementation
    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let docs = self.documents.read().await;
        let text = match docs.get(&params.text_document.uri) {
            Some(t) => t,
            None => return Ok(None),
        };

        if let Ok(file) = parse_file(text) {
            let symbols = build_document_symbols(&file);
            return Ok(Some(DocumentSymbolResponse::Nested(symbols)));
        }
        
        Ok(None)
    }
}
