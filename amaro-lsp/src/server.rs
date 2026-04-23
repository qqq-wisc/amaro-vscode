use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::ast::*;
use crate::info::builtins;
use crate::parser::symbols::{Type, UserDefTable};
use crate::parser::{SemanticResult, StringLabels, check_semantics, parse_file, semantics, utils};

#[derive(Debug)]
pub struct CachedParse {
    file: AmaroFile,
    type_map: HashMap<NodeId, Type>,
    user_def_table: UserDefTable,
    string_labels: StringLabels,
}

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub documents: Arc<RwLock<HashMap<Url, String>>>,
    pub parse_cache: Arc<RwLock<HashMap<Url, CachedParse>>>,
}

// Symbol Tree Builder
pub fn build_document_symbols(file: &AmaroFile) -> Vec<DocumentSymbol> {
    file.blocks
        .iter()
        .map(|block| {
            let kind = match block.kind.as_str() {
                "GateRealization" | "Transition" | "Architecture" | "Arch" => SymbolKind::CLASS,
                "Step" => SymbolKind::FUNCTION,
                "RouteInfo" | "TransitionInfo" | "ArchInfo" | "StateInfo" => SymbolKind::MODULE,
                _ => SymbolKind::OBJECT,
            };

            #[allow(deprecated)]
            let children: Vec<DocumentSymbol> = match &block.content {
                BlockContent::Fields(items) => items
                    .iter()
                    .map(|item| match item {
                        BlockItem::Field(field) => DocumentSymbol {
                            name: field.key.clone(),
                            detail: Some(format_expr_preview(&field.value)),
                            kind: SymbolKind::FIELD,
                            tags: None,
                            deprecated: None,
                            range: field.key_range,
                            selection_range: field.key_range,
                            children: None,
                        },
                        BlockItem::StructDef(struct_def) => DocumentSymbol {
                            name: struct_def.name.clone(),
                            detail: Some(format!("Struct with {} fields", struct_def.fields.len())),
                            kind: SymbolKind::STRUCT,
                            tags: None,
                            deprecated: None,
                            range: struct_def.range,
                            selection_range: struct_def.name_range,
                            children: None,
                        },
                        BlockItem::ReturnKeyword { range, key } => DocumentSymbol {
                            name: key.clone(),
                            detail: Some("(invalid: 'return' in expression context)".to_string()),
                            kind: SymbolKind::FIELD,
                            tags: None,
                            deprecated: None,
                            range: *range,
                            selection_range: *range,
                            children: None,
                        },
                    })
                    .collect(),
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
                children: if children.is_empty() {
                    None
                } else {
                    Some(children)
                },
            }
        })
        .collect()
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
        ExprKind::FunctionCall { function, .. } => {
            format!("{}(...)", format_expr_preview(function))
        }

        ExprKind::FieldAccess { object, field } => {
            format!("{}.{}", format_expr_preview(object), field)
        }
        ExprKind::IndexAccess { object, .. } => format!("{}[...]", format_expr_preview(object)),

        // Handle Projections (e.g., tuple.(0))
        ExprKind::Projection { index, .. } => format!("tuple.({})", index),

        ExprKind::Lambda { .. } => "|...| -> ...".to_string(),
        ExprKind::IfThenElse { .. } => "if ... then ...".to_string(),
        ExprKind::LetBinding { name, .. } => format!("let {} = ...", name.string),

        ExprKind::BinaryOp { .. } => "expr op expr".to_string(),
        ExprKind::UnaryOp { op, operand } => format!("{:?} {}", op, format_expr_preview(operand)),
        ExprKind::TensorProduct { .. } => "... ⊗ ...".to_string(),

        ExprKind::Some(_) => "Some(...)".to_string(),
        ExprKind::None => "None".to_string(),

        ExprKind::Match { scrutinee, arms } => {
            format!(
                "match {} with ({} arms)",
                format_expr_preview(scrutinee),
                arms.len()
            )
        }
    }
}

#[allow(dead_code)]
#[cfg(debug_assertions)]
fn format_simple_ast(file: &AmaroFile) -> String {
    let mut output = String::new();
    output.push_str("=== AST Summary ===\n");
    for block in &file.blocks {
        let start = block.range.start;
        output.push_str(&format!(
            "Amaro Block: {} at line {}, col {}\n",
            block.kind,
            start.line + 1,
            start.character
        ));

        match &block.content {
            BlockContent::Fields(items) => {
                for item in items {
                    match item {
                        BlockItem::Field(f) => {
                            let key_pos = f.key_range.start;
                            output.push_str(&format!(
                                "  Field: {} = {} (line {}, col {})\n",
                                f.key,
                                summarize_expr(&f.value),
                                key_pos.line + 1,
                                key_pos.character
                            ));
                        }
                        BlockItem::StructDef(s) => {
                            let struct_pos = s.name_range.start;
                            output.push_str(&format!(
                                "  StructDef: {} (line {}, col {})\n",
                                s.name,
                                struct_pos.line + 1,
                                struct_pos.character
                            ));
                        }
                        BlockItem::ReturnKeyword { key, .. } => {
                            output.push_str(&format!(
                                "  ReturnKeyword: {} = return ... (invalid)\n",
                                key
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

#[allow(dead_code)]
#[cfg(debug_assertions)]
fn summarize_expr(expr: &Expr) -> String {
    summarize_expr_detailed(expr, 0)
}

#[allow(dead_code)]
#[cfg(debug_assertions)]
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
                let contents = items
                    .iter()
                    .map(|e| summarize_expr_detailed(e, depth + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{}]", contents)
            } else {
                format!(
                    "[{} items: {}, ...]",
                    items.len(),
                    summarize_expr_detailed(&items[0], depth + 1)
                )
            }
        }

        ExprKind::Tuple(items) => {
            if items.len() <= 2 {
                let contents = items
                    .iter()
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
                let field_strs = fields
                    .iter()
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
                format!(
                    "{}({})",
                    func_name,
                    summarize_expr_detailed(&args[0], depth + 1)
                )
            } else if args.len() == 2 {
                format!(
                    "{}({}, {})",
                    func_name,
                    summarize_expr_detailed(&args[0], depth + 1),
                    summarize_expr_detailed(&args[1], depth + 1)
                )
            } else {
                format!("{}({} args)", func_name, args.len())
            }
        }

        ExprKind::FieldAccess { object, field } => {
            format!("{}.{}", summarize_expr_detailed(object, depth + 1), field)
        }

        ExprKind::IndexAccess { object, index } => {
            format!(
                "{}[{}]",
                summarize_expr_detailed(object, depth + 1),
                summarize_expr_detailed(index, depth + 1)
            )
        }

        ExprKind::Projection { index, tuple } => {
            format!("{}.({})", summarize_expr_detailed(tuple, depth + 1), index)
        }

        ExprKind::Lambda { params, body } => {
            if depth < 2 {
                format!(
                    "|{}| -> {}",
                    params
                        .iter()
                        .map(|s| s.string.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    summarize_expr_detailed(body, depth + 1)
                )
            } else {
                format!(
                    "|{}| -> ...",
                    params
                        .iter()
                        .map(|s| s.string.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }

        ExprKind::IfThenElse {
            condition,
            then_branch,
            else_branch,
        } => {
            if depth == 0 {
                format!(
                    "if {}\n      then {}\n      else {}",
                    summarize_expr_detailed(condition, depth + 1),
                    summarize_expr_detailed(then_branch, depth + 1),
                    summarize_expr_detailed(else_branch, depth + 1)
                )
            } else {
                format!(
                    "if {} then {} else {}",
                    summarize_expr_detailed(condition, depth + 1),
                    summarize_expr_detailed(then_branch, depth + 1),
                    summarize_expr_detailed(else_branch, depth + 1)
                )
            }
        }

        ExprKind::LetBinding { name, value, body } => {
            if depth == 0 {
                format!(
                    "let {} = {}\n      in {}",
                    name.string,
                    summarize_expr_detailed(value, depth + 1),
                    summarize_expr_detailed(body, depth + 1)
                )
            } else {
                format!(
                    "let {} = {} in {}",
                    name.string,
                    summarize_expr_detailed(value, depth + 1),
                    summarize_expr_detailed(body, depth + 1)
                )
            }
        }

        ExprKind::BinaryOp { op, left, right } => {
            format!(
                "({} {:?} {})",
                summarize_expr_detailed(left, depth + 1),
                op,
                summarize_expr_detailed(right, depth + 1)
            )
        }

        ExprKind::UnaryOp { op, operand } => {
            format!("{:?}({})", op, summarize_expr_detailed(operand, depth + 1))
        }

        ExprKind::TensorProduct { left, right } => {
            format!(
                "{} ⊗ {}",
                summarize_expr_detailed(left, depth + 1),
                summarize_expr_detailed(right, depth + 1)
            )
        }

        ExprKind::Match { scrutinee, arms } => {
            format!(
                "match {} with {} arms",
                summarize_expr_detailed(scrutinee, depth + 1),
                arms.len()
            )
        }
    }
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Backend {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            parse_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // Validating Document
    pub async fn validate_document(&self, uri: Url, text: String) {
        let mut diagnostics = Vec::new();

        // Syntactic Analysis
        match parse_file(&text) {
            Ok(file) => {
                // Semantic Checks
                // #[cfg(debug_assertions)]
                // {
                //     let ast_summary = format_simple_ast(&file);
                //     self.client.log_message(MessageType::INFO, format!("Parsed AST:\n{}", ast_summary)).await;
                // }

                let SemanticResult {
                    diagnostics: mut semantic_errors,
                    type_map,
                    user_def_table,
                    string_labels
                } = check_semantics(&file);

                self.parse_cache.write().await.insert(
                    uri.clone(),
                    CachedParse {
                        file,
                        type_map,
                        user_def_table,
                        string_labels,
                    },
                );
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

        self.client
            .publish_diagnostics(uri, diagnostics, Some(1))
            .await;
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
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec!['.'.to_string(), '#'.to_string()]),
                    ..Default::default()
                }),

                hover_provider: Some(HoverProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),

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

        self.documents
            .write()
            .await
            .insert(uri.clone(), text.clone());

        // self.client
        //     .log_message(MessageType::INFO, "Amaro file opened!")
        //     .await;
        self.validate_document(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        if let Some(change) = params.content_changes.into_iter().next() {
            let text = change.text.clone();
            self.documents
                .write()
                .await
                .insert(uri.clone(), text.clone());

            self.validate_document(uri, text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], Some(1))
            .await;
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

    // triggers autocomplete
    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        // get the text doc
        let uri = params.text_document_position.text_document.uri;
        let content_guard = self.documents.read().await;
        let file_content = content_guard.get(&uri);

        if file_content.is_none() {
            return Err(Error::new(ErrorCode::InvalidRequest));
        }
        let string_content = file_content.unwrap();

        // get original position of cursor. this is after the dot.
        let orig_pos = params.text_document_position.position;

        let char_before_pos = Position::new(orig_pos.line, orig_pos.character.saturating_sub(1));

        // determine if . was typed
        match utils::get_char_at(string_content, char_before_pos) {
            None => Ok(None),
            Some('.') => self.dot_autocomplete(uri, char_before_pos).await, // matched the dot!
            Some('#') => self.hash_autocomplete(char_before_pos),
            Some(_) => Ok(None),
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        // TODO make this more efficient.
        // right now, we only cache the text and not the expressions & stuff.
        // this is a lot of repeated work if the user changes hover often

        // get the text doc
        let uri = params.text_document_position_params.text_document.uri;

        let guard = self.parse_cache.read().await;

        let (amaro_file, type_map, string_labels) = match guard.get(&uri) {
            None => return Err(Error::new(ErrorCode::InvalidRequest)),
            Some(t) => (&t.file, &t.type_map, &t.string_labels),
        };

        // get original position of cursor. this is after the dot.
        let orig_pos = params.text_document_position_params.position;

        

        // first, check field names.
        // this is necessary to come before the strings for hover, since fields
        // are also in the string_labels section.
        let hovered_field = utils::field_name_containing(amaro_file, orig_pos);

        if let Some((block_name, field_name, field_range)) = hovered_field {
            // need to lookup the name
            if let Some(field_info) =
                builtins::field_lookup(block_name.as_str(), field_name.as_str())
            {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: field_info.show_details(),
                    }),
                    range: Some(field_range),
                }));
            } else {
                // we are done. if we are hovered over a field but we don't
                // have an entry, then there's nothing to show...
                // LS: Would be good to display something indicating that the
                // thing we are hovering over is not recognized by the program
                return Ok(None);
            }
        }


        // second, check the strings for hover (for instance, the labels in
        // lambdas)
        let hovered_string = string_labels.get_label_info(&orig_pos);

        if let Some((range, _, typ)) = hovered_string {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: typ.to_markdown_display(),
                }),
                range: Some(range),
            }));
        }

        // find the largest expression containing this position before the dot
        let containing_expr = utils::smallest_expr_containing(amaro_file, orig_pos);

        match containing_expr {
            // if we had an expression containing our goal pos...
            Ok(e) => match type_map.get(&e.id) {
                None => Ok(None),
                Some(t) => Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: t.to_markdown_display(),
                    }),
                    range: Some(e.range),
                })),
            },
            // if we lacked an expression containing our goal pos...
            Err(_) => {
                // self.client
                //     .log_message(
                //         MessageType::INFO,
                //         "\tNo expression found containing the position.".to_string(),
                //     )
                //     .await;
                Ok(None) //
            }
        }
    }

    // provides the "rust analyzer" style hints for let and lambdas
    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;

        let guard = self.parse_cache.read().await;

        eprintln!("Checking guard for string labels");

        let string_labels = match guard.get(&uri) {
            None => return Ok(None),
            Some(t) => &t.string_labels,
        };

        eprintln!("Got string labels.");

        let res: Vec<InlayHint> = string_labels
            .get_all_labels_in_range(&params.range)
            .iter()
            .map(|elt| InlayHint {
                position: elt.0.end,
                label: InlayHintLabel::String(format!(": {}", elt.2)), // todo change this
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: Some(true),
                padding_right: Some(true),
                data: None,
            })
            .collect();
        Ok(Some(res))
    }
}

impl Backend {
    /// Autocomplete resolver for typing '.'
    async fn dot_autocomplete(
        &self,
        uri: Url,
        dot_pos: Position,
    ) -> Result<Option<CompletionResponse>> {
        // so, we have [stuff][.][cursor]
        // we need to look 1 char before
        let before_dot_pos = Position::new(dot_pos.line, dot_pos.character.saturating_sub(1));

        let parse_guard = self.parse_cache.read().await;
        let cached_parse = match parse_guard.get(&uri) {
            None => return Err(Error::new(ErrorCode::InvalidRequest)),
            Some(p) => p,
        };
        // find the largest expression containing this position before the dot
        let containing_expr = utils::largest_expr_containing(&cached_parse.file, before_dot_pos);

        match containing_expr {
            // if we had an expression containing our goal pos...
            Ok(e) => {
                // now, need to find the expr right before our cursor, and get
                // the type of that one.
                match utils::find_finishing_subexpr(e, dot_pos) {
                    Some(perfect_end_expr) => {
                        // self.client
                        //     .log_message(
                        //         MessageType::INFO,
                        //         format!("\tFound perfectly finishing expression {}", perfect_end_expr.kind),
                        //     )
                        //     .await;

                        // get the types and stuff
                        let found_type = match cached_parse.type_map.get(&perfect_end_expr.id) {
                            Some(t) => t,
                            None => return Err(Error::new(ErrorCode::InternalError)),
                        };

                        // self.client
                        //     .log_message(
                        //         MessageType::INFO,
                        //         format!("\tHas type {:?}", found_type),
                        //     )
                        //     .await;

                        match semantics::suggest_next_from_type(
                            found_type,
                            &cached_parse.user_def_table,
                        ) {
                            Some(suggestions) => Ok(Some(CompletionResponse::Array(suggestions))),
                            None => Ok(None), // don't show suggestions then!
                        }
                    }
                    // expression doesn't end at anything
                    None => Ok(None),
                }
            }
            // if we lacked an expression containing our goal pos...
            Err(_) => {
                // self.client
                //     .log_message(
                //         MessageType::INFO,
                //         "\tNo expression found containg the position.".to_string(),
                //     )
                //     .await;
                Ok(None) //
            }
        }
    }
    fn hash_autocomplete(&self, hash_pos: Position) -> Result<Option<CompletionResponse>> {
        let end_pos = Position::new(hash_pos.line, hash_pos.character.saturating_add(1));
        let range = Range::new(hash_pos, end_pos);
        let completion_item_vec: Vec<CompletionItem> = builtins::get_all_raw_built_ins()
            .iter()
            .map(|elt| {
                elt.to_completion_item(Some(vec![TextEdit {
                    range,
                    new_text: "".to_string(),
                }]))
            })
            .collect();
        Ok(Some(CompletionResponse::Array(completion_item_vec)))
    }
}
