use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};
use crate::ast::AmaroFile;

// Semantic Analysis
pub fn check_semantics(file: &AmaroFile) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let known_blocks = [
        "GateRealization", "Transition", "Architecture", "Arch", "Step",
        "RouteInfo", "TransitionInfo", "ArchInfo", "StateInfo"
    ];

    for block in &file.blocks {
        // Rule: Warn if block name is lowercase (Convention check)
        if let Some(correct_name) = known_blocks.iter().find(|&&kb| kb.eq_ignore_ascii_case(&block.kind)) {
            if &block.kind != *correct_name {
                diagnostics.push(Diagnostic {
                    range: block.range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!("Block '{}' should be Capitalized (e.g., '{}').", block.kind, correct_name),
                    ..Default::default()
                });
            }
        }
    }

    diagnostics
}
