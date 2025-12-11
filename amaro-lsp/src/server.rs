use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use crate::parser::{parse_file, check_semantics};

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
}

impl Backend {
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
    async fn initialize(&self, _: InitializeParams) -> tower_lsp::jsonrpc::Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "Amaro file opened!")
            .await;
        self.validate_document(
            params.text_document.uri,
            params.text_document.text
        ).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            self.validate_document(
                params.text_document.uri,
                change.text
            ).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.client.publish_diagnostics(
            params.text_document.uri,
            vec![],
            Some(1)
        ).await;
    }

    async fn shutdown(&self) -> tower_lsp::jsonrpc::Result<()> {
        Ok(())
    }
}
