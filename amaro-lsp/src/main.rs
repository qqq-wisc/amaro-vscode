use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use nom::error::Error;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while, take_while1},
    character::complete::{char, multispace1, not_line_ending, satisfy},
    combinator::{peek, recognize},
    multi::many0,
    sequence::{delimited, pair},
    IResult,
};

#[derive(Debug)]
struct AmaroFile {
    blocks: Vec<Block>,
}

#[derive(Debug)]
struct Block {
    kind: String,
    range: Range,
}

#[derive(Debug)]
struct Backend {
    client: Client,
}

impl Backend {
    // Validating Document
    async fn validate_document(&self, uri: Url, text: String) {
        let mut diagnostics = Vec::new();

        // Syntactic Analysis
        match parse_file(&text) {
            Ok(file) => {
                // Semantic Checks
                let mut semantic_errors = self.check_semantics(&file);
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

    // Semantic Analysis
    fn check_semantics(&self, file: &AmaroFile) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let known_blocks = [
            "GateRealization", "Transition", "Architecture", "Arch", "Step",
            "RouteInfo", "TransitionInfo", "ArchInfo", "StateInfo"
        ];

        for block in &file.blocks {
            // Rule: Warn if block name is lowercase (Convention check)
            if known_blocks.contains(&block.kind.as_str()) {
                let first_char = block.kind.chars().next().unwrap_or(' ');
                if first_char.is_lowercase() {
                    diagnostics.push(Diagnostic {
                        range: block.range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!("Block '{}' should be Capitalized.", block.kind),
                        ..Default::default()
                    });
                }
            }
        }

        diagnostics
    }
}


// NOM Parsing
fn whitespace_handler(input: &str) -> IResult<&str, &str> {
    recognize(many0(alt((
        multispace1,
        recognize(pair(tag("//"), not_line_ending)),
    ))))(input)
}

fn parse_rust_embedded(input: &str) -> IResult<&str, &str> {
    // CHANCE OF BREAKING if the embedded rust program is of the form {{ program }}
    recognize(delimited(tag("{{"), take_until("}}"), tag("}}")))(input)
}


fn parse_identifier(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        satisfy(|c| c.is_alphabetic() || c == '_'),
        take_while(|c: char| c.is_alphanumeric() || c == '_')
    ))(input)
}

fn parse_balanced_parenthesis(input: &str) -> IResult<&str, &str> {
    recognize(many0(alt((
        parse_rust_embedded,
        delimited(char('['), parse_balanced_parenthesis, char(']')),
        delimited(char('{'), parse_balanced_parenthesis, char('}')),
        delimited(char('('), parse_balanced_parenthesis, char(')')),

        take_while1(|c| c != '[' && c != ']' && c != '{' && c != '}' && c != '(' && c != ')')
    ))))(input)
}

fn parse_block<'a>(original_input: &'a str, input: &'a str) -> IResult<&'a str, Option<Block>> {
    let (input, _) = whitespace_handler(input)?;

    let start_offset = input.as_ptr() as usize - original_input.as_ptr() as usize;

    let (input, kind) = parse_identifier(input)?;
    let (input, _) = whitespace_handler(input)?;

    let check_colon: IResult<&str, char, Error<&str>> = peek(char(':'))(input);

    if check_colon.is_ok() {
        let (input, _) = char(':')(input)?;

        return Ok((input, Some(Block {
            kind: kind.to_string(),
            range: calc_range(original_input, start_offset, kind.len()),
        })));
    }

    let (input, _) = char::<&str, Error<&str>>('[')(input)?;
    
    let (input, _content) = parse_balanced_parenthesis(input)?;
    let (rest, _) = char(']')(input)?;

    Ok((rest, Some(Block {
        kind: kind.to_string(),
        range: calc_range(original_input, start_offset, kind.len()),
    })))
}

fn parse_file(input: &str) -> std::result::Result<AmaroFile, ()> {
    let mut blocks = Vec::new();
    let mut current_input = input;

    while !current_input.is_empty() {
        match parse_block(input, current_input) {
            Ok((rest, maybe_block)) => {
                if let Some(block) = maybe_block {
                    blocks.push(block);
                }
                current_input = rest;
            },
            Err(_) => {
                if let Some(pos) = current_input.find('\n') {
                    current_input = &current_input[pos + 1..];
                } else {
                    current_input = "";
                }
            }
        }
    }

    Ok(AmaroFile { blocks })
}


// UTLS
fn calc_range(full_text: &str, start_offset: usize, length: usize) -> Range {
    let abs_start = start_offset;
    let abs_end = start_offset + length;
    
    let (start_line, s_col) = byte_to_position(full_text, abs_start);
    let (end_line, e_col) = byte_to_position(full_text, abs_end);

    Range {
        start: Position { line: start_line, character: s_col },
        end: Position { line: end_line, character: e_col },
    }
}

fn byte_to_position(text: &str, byte_idx: usize) -> (u32, u32) {
    let safe_idx = std::cmp::min(byte_idx, text.len());
    let slice = &text[..safe_idx];
    let line = slice.matches('\n').count() as u32;
    let last_line_start = slice.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = (safe_idx - last_line_start) as u32;
    (line, col)
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
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

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
