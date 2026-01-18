use tower_lsp::lsp_types::{
    Diagnostic,
    DiagnosticSeverity,
    DiagnosticRelatedInformation,
    Location,
    Range,
    Url
};
use std::collections::HashMap;
use crate::ast::AmaroFile;

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
                            uri: Url::parse("file:///previous/definition").unwrap(),
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
            let present_keys: Vec<&str> = block.fields.iter().map(|f| f.key.as_str()).collect();
            
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
    }

    // 4. Mandatory Blocks Check
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
