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

// Symbol Tree Builder
pub fn build_document_symbols(file: &AmaroFile) -> Vec<DocumentSymbol> {
    file.blocks.iter().map(|block| {
        let kind = match block.kind.as_str() {
            "GateRealization" | "Transition" | "Architecture" | "Arch" => SymbolKind::CLASS,
            "Step" => SymbolKind::FUNCTION,
            "RouteInfo" | "TransitionInfo" | "ArchInfo" | "StateInfo" => SymbolKind::MODULE,
            _ => SymbolKind::OBJECT,
        };

        #[allow(deprecated)]
        let children: Vec<DocumentSymbol> = match &block.content {
            BlockContent::Fields(items) => items.iter().filter_map(|item| {
                match item {
                    BlockItem::Field(field) => {
                        Some(DocumentSymbol {
                            name: field.key.clone(),
                            detail: Some(format_expr_preview(&field.value)),
                            kind: SymbolKind::FIELD,
                            tags: None,
                            deprecated: None,
                            range: field.key_range,
                            selection_range: field.key_range,
                            children: None,
                        })
                    },
                    BlockItem::StructDef(struct_def) => {
                        Some(DocumentSymbol {
                            name: struct_def.name.clone(),
                            detail: Some(format!("Struct with {} fields", struct_def.fields.len())),
                            kind: SymbolKind::STRUCT,
                            tags: None,
                            deprecated: None,
                            range: struct_def.range,
                            selection_range: struct_def.name_range,
                            children: None,
                        })
                    },
                }
            }).collect(),
        };

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

fn format_expr_preview(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Identifier(name) => name.clone(),
        ExprKind::IntLiteral(i) => i.to_string(),
        ExprKind::FloatLiteral(f) => format!("{:.2}", f),
        ExprKind::StringLiteral(s) => format!("'{}'", s),
        ExprKind::BoolLiteral(b) => b.to_string(),
        
        ExprKind::List(items) => format!("[{} items]", items.len()),
        ExprKind::Tuple(items) => format!("({} items)", items.len()),
        
        ExprKind::StructLiteral { name, .. } => format!("{} {{...}}", name),
        ExprKind::FunctionCall { function, .. } => format!("{}(...)", format_expr_preview(function)),
        
        ExprKind::FieldAccess { object, field } => format!("{}.{}", format_expr_preview(object), field),
        ExprKind::IndexAccess { object, .. } => format!("{}[...]", format_expr_preview(object)),
        
        // Handle Projections (e.g., tuple.(0))
        ExprKind::Projection { index, .. } => format!("tuple.({})", index),

        ExprKind::Lambda { .. } => "|...| -> ...".to_string(),
        ExprKind::IfThenElse { .. } => "if ... then ...".to_string(),
        ExprKind::LetBinding { name, .. } => format!("let {} = ...", name),
        
        ExprKind::BinaryOp { .. } => "expr op expr".to_string(),
        ExprKind::UnaryOp { op, operand } => format!("{:?} {}", op, format_expr_preview(operand)),
        ExprKind::TensorProduct { .. } => "... ⊗ ...".to_string(),

        ExprKind::Some(_) => "Some(...)".to_string(),
        ExprKind::None => "None".to_string(),
    }
}

fn format_simple_ast(file: &AmaroFile) -> String {
    let mut output = String::new();
    output.push_str("=== AST Summary ===\n");
    for block in &file.blocks {
        let start = block.range.start;
        output.push_str(&format!("Amaro Block: {} at line {}, col {}\n", block.kind, start.line + 1, start.character));

        match &block.content {
            BlockContent::Fields(items) => {
                for item in items {
                    match item {
                        BlockItem::Field(f) => {
                            let key_pos = f.key_range.start;
                            output.push_str(&format!("  Field: {} = {} (line {}, col {})\n", 
                                f.key, 
                                summarize_expr(&f.value), 
                                key_pos.line + 1, 
                                key_pos.character
                            ));
                        }
                        BlockItem::StructDef(s) => {
                            let struct_pos = s.name_range.start;
                            output.push_str(&format!("  StructDef: {} (line {}, col {})\n", 
                                s.name, 
                                struct_pos.line + 1, 
                                struct_pos.character
                            ));
                        }
                    }
                }
            }
        }
    }
    output.push_str("===================\n\n");
    output
}

fn summarize_expr(expr: &Expr) -> String {
    summarize_expr_detailed(expr, 0)
}

fn summarize_expr_detailed(expr: &Expr, depth: usize) -> String {
    if depth > 3 {
        return "...".to_string();
    }
    
    match &expr.kind {
        ExprKind::Identifier(s) => s.clone(),
        ExprKind::FloatLiteral(f) => format!("{}", f),
        ExprKind::IntLiteral(i) => format!("{}", i),
        ExprKind::BoolLiteral(b) => format!("{}", b),
        ExprKind::StringLiteral(s) => format!("'{}'", s),
        
        ExprKind::List(items) => {
            if items.is_empty() {
                "[]".to_string()
            } else if items.len() <= 3 {
                let contents = items.iter()
                    .map(|e| summarize_expr_detailed(e, depth + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{}]", contents)
            } else {
                format!("[{} items: {}, ...]", items.len(), summarize_expr_detailed(&items[0], depth + 1))
            }
        }
        
        ExprKind::Tuple(items) => {
            if items.len() <= 2 {
                let contents = items.iter()
                    .map(|e| summarize_expr_detailed(e, depth + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({})", contents)
            } else {
                format!("({} items)", items.len())
            }
        }
        
        ExprKind::Some(inner) => {
            format!("Some({})", summarize_expr_detailed(inner, depth + 1))
        }
        
        ExprKind::None => "None".to_string(),
        
        ExprKind::StructLiteral { name, fields } => {
            if fields.is_empty() {
                format!("{}{{}}", name)
            } else if fields.len() <= 2 {
                let field_strs = fields.iter()
                    .map(|(k, v)| format!("{} = {}", k, summarize_expr_detailed(v, depth + 1)))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}{{{}}}", name, field_strs)
            } else {
                format!("{}{{... {} fields}}", name, fields.len())
            }
        }
        
        ExprKind::FunctionCall { function, args } => {
            let func_name = summarize_expr_detailed(function, depth + 1);
            if args.is_empty() {
                format!("{}()", func_name)
            } else if args.len() == 1 {
                format!("{}({})", func_name, summarize_expr_detailed(&args[0], depth + 1))
            } else if args.len() == 2 {
                format!("{}({}, {})", 
                    func_name, 
                    summarize_expr_detailed(&args[0], depth + 1),
                    summarize_expr_detailed(&args[1], depth + 1))
            } else {
                format!("{}({} args)", func_name, args.len())
            }
        }
        
        ExprKind::FieldAccess { object, field } => {
            format!("{}.{}", summarize_expr_detailed(object, depth + 1), field)
        }
        
        ExprKind::IndexAccess { object, index } => {
            format!("{}[{}]", 
                summarize_expr_detailed(object, depth + 1),
                summarize_expr_detailed(index, depth + 1))
        }
        
        ExprKind::Projection { index, tuple } => {
            format!("{}.({})", summarize_expr_detailed(tuple, depth + 1), index)
        }
        
        ExprKind::Lambda { params, body } => {
            if depth < 2 {
                format!("|{}| -> {}", params.join(", "), summarize_expr_detailed(body, depth + 1))
            } else {
                format!("|{}| -> ...", params.join(", "))
            }
        }
        
        ExprKind::IfThenElse { condition, then_branch, else_branch } => {
            if depth == 0 {
                format!("if {}\n      then {}\n      else {}", 
                    summarize_expr_detailed(condition, depth + 1),
                    summarize_expr_detailed(then_branch, depth + 1),
                    summarize_expr_detailed(else_branch, depth + 1))
            } else {
                format!("if {} then {} else {}", 
                    summarize_expr_detailed(condition, depth + 1),
                    summarize_expr_detailed(then_branch, depth + 1),
                    summarize_expr_detailed(else_branch, depth + 1))
            }
        }
        
        ExprKind::LetBinding { name, value, body } => {
            if depth == 0 {
                format!("let {} = {}\n      in {}", 
                    name,
                    summarize_expr_detailed(value, depth + 1),
                    summarize_expr_detailed(body, depth + 1))
            } else {
                format!("let {} = {} in {}", 
                    name,
                    summarize_expr_detailed(value, depth + 1),
                    summarize_expr_detailed(body, depth + 1))
            }
        }
        
        ExprKind::BinaryOp { op, left, right } => {
            format!("({} {:?} {})", 
                summarize_expr_detailed(left, depth + 1),
                op,
                summarize_expr_detailed(right, depth + 1))
        }
        
        ExprKind::UnaryOp { op, operand } => {
            format!("{:?}({})", op, summarize_expr_detailed(operand, depth + 1))
        }
        
        ExprKind::TensorProduct { left, right } => {
            format!("{} ⊗ {}", 
                summarize_expr_detailed(left, depth + 1),
                summarize_expr_detailed(right, depth + 1))
        }
    }
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
                let ast_summary = format_simple_ast(&file);
                self.client.log_message(MessageType::INFO, format!("Parsed AST:\n{}", ast_summary)).await;
                // let ast_debug = format!("{:#?}", file);
                // self.client.log_message(MessageType::INFO, format!("Parsed AST:\n{}", ast_debug)).await;
                
                let mut semantic_errors = check_semantics(&file);
                diagnostics.append(&mut semantic_errors);
            }
            Err(e) => {
                diagnostics.push(Diagnostic {
                    range: Range::default(),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("Fatal Syntax Error: Parsing aborted.\nParse error: {}", e),
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
