use tower_lsp::lsp_types::{
    Diagnostic,
    DiagnosticSeverity,
    DiagnosticRelatedInformation,
    Location,
    Range,
    Url
};
use std::collections::HashMap;
use crate::ast::*;

// Semantic Analysis
pub fn check_semantics(file: &AmaroFile) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let known_blocks = [
        "GateRealization", "Transition", "Architecture", "Arch", "Step",
        "RouteInfo", "TransitionInfo", "ArchInfo", "StateInfo"
    ];

    // Define which keys are REQUIRED inside specific blocks
    let mut required_keys: HashMap<&str, Vec<&str>> = HashMap::new();
    required_keys.insert("RouteInfo", vec!["routed_gates", "realize_gate"]);
    required_keys.insert("TransitionInfo", vec!["cost", "apply"]);

    required_keys.insert("ArchInfo", vec![]);
    required_keys.insert("StateInfo", vec![]);

    let mut found_blocks: HashMap<String, Range> = HashMap::new();

    for block in &file.blocks {
        let block_name = block.kind.as_str();
        let lower_name = block_name.to_lowercase();

        // 1. Capitalization Check
        if let Some(correct_name) = known_blocks.iter().find(|&&kb| kb.eq_ignore_ascii_case(block_name)) {
            if block_name != *correct_name {
                diagnostics.push(Diagnostic {
                    range: block.range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("Block '{}' should be Capitalized (e.g., '{}').", block_name, correct_name),
                    ..Default::default()
                });
            }
        }

        // 2. Uniqueness Check
        if let Some(first_range) = found_blocks.get(&lower_name) {
            diagnostics.push(Diagnostic {
                range: block.range,
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Duplicate definition of '{}' block.", block_name),
                related_information: Some(vec![
                    DiagnosticRelatedInformation {
                        location: Location {
                            uri: Url::parse("file:///previous/definition").unwrap_or_else(|_| Url::parse("file:///unknown").unwrap()),
                            range: *first_range
                        },
                        message: "First defined here".to_string()
                    }
                ]),
                ..Default::default()
            });
        } else {
            found_blocks.insert(lower_name, block.range);
        }

        // 3. Required Keys Check
        // This checks if 'RouteInfo' actually contains 'routed_gates'
        if let Some(reqs) = required_keys.get(block_name) {
            let present_keys: Vec<&str> = match &block.content {
                BlockContent::Fields(items) => items.iter().filter_map(|item| {
                    if let BlockItem::Field(field) = item {
                        Some(field.key.as_str())
                    } else {
                        None
                    }
                }).collect(),
            };
           
            for req in reqs {
                if !present_keys.contains(req) {
                    diagnostics.push(Diagnostic {
                        range: block.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!("Block '{}' is missing required field: '{}'", block_name, req),
                        ..Default::default()
                    });
                }
            }
        }

        // 4. Gate Validation in 'routed_gates' fields
        if block_name == "RouteInfo" {
            let BlockContent::Fields(items) = &block.content; 
            for item in items {
                if let BlockItem::Field(field) = item {
                    if field.key == "routed_gates" {
                        validate_gates(&field.value, &mut diagnostics);
                    }
                }
            }
        }
    }

    // 5. Mandatory Blocks Check
    let required_blocks = ["RouteInfo", "TransitionInfo"];
    for req in required_blocks {
        if !found_blocks.contains_key(&req.to_lowercase()) {
             diagnostics.push(Diagnostic {
                range: Range::default(),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Missing mandatory block: '{}'.", req),
                ..Default::default()
            });
        }
    }

    diagnostics
}

fn validate_gates(expr: &Expr, diagnostics: &mut Vec<Diagnostic>) {
    let valid_gates = ["CX", "T", "Pauli", "PauliMeasurement"];

    match &expr.kind {
        ExprKind::Identifier(name) => {
            if !valid_gates.contains(&name.as_str()) {
                diagnostics.push(Diagnostic {
                    range: expr.range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("'{}' is not a recognized standard gate. Expected one of: {:?}", name, valid_gates),
                    ..Default::default()
                });
            }
        },
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                validate_gates(item, diagnostics);
            }
        },
        _ => {}
    }
}
