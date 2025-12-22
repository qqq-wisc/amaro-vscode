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

    let mut found_blocks: HashMap<String, Range> = HashMap::new();

   
    for block in &file.blocks {
        // 1. Capitalization
        let block_name = block.kind.as_str();
        let lower_name = block_name.to_lowercase();

        // Rule: Warn if block name is lowercase (Convention check)
        if let Some(correct_name) = known_blocks.iter().find(|&&kb| kb.eq_ignore_ascii_case(block_name)) {
            if block_name != *correct_name {
                diagnostics.push(Diagnostic {
                    range: block.range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("Block '{}' should be Capitalized (e.g., '{}').", block.kind, correct_name),
                    ..Default::default()
                });
            }
        }

        // 2. Uniqueness
        if let Some(first_range) = found_blocks.get(&lower_name) {
            diagnostics.push(Diagnostic {
                range: block.range,
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Duplicate definition of '{}' block.", block_name),
                related_information: Some(vec![
                    DiagnosticRelatedInformation {
                        location: Location {
                            uri: Url::parse("file:///previous/definition").unwrap(), // Hint only
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
    }


    // 3. Mandatory Blocks Check
    let required = ["RouteInfo", "TransitionInfo"];
    for req in required {
        if !found_blocks.contains_key(&req.to_lowercase()) {
             diagnostics.push(Diagnostic {
                range: Range::default(),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Missing mandatory block: '{}'. Block required as per Amaro grammar.", req),
                ..Default::default()
            });
        }
    }

    diagnostics
}
