use amaro_lsp::parser::{parse_file, check_semantics};
use tower_lsp::lsp_types::DiagnosticSeverity;

const MOCK_MANDATORY_BLOCKS: &str = "RouteInfo:\nTransitionInfo:\n";

#[test]
fn capitalization_warning() {
    let input = format!("{}{}", MOCK_MANDATORY_BLOCKS, "architecture[1]");
    let file = parse_file(&input).unwrap();
    let diags = check_semantics(&file);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("Capitalized"));
}

#[test]
fn no_warning_for_correct_capitalization() {
    let input = format!("{}{}", MOCK_MANDATORY_BLOCKS, "Architecture[1]");
    let file = parse_file(&input).unwrap();
    let diags = check_semantics(&file);
    assert!(diags.is_empty());
}

#[test]
fn test_missing_mandatory_blocks() {
    let input = "Architecture[1]"; 
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    assert_eq!(diags.len(), 2);
    assert!(diags.iter().any(|d| d.message.contains("Missing mandatory block: 'RouteInfo'")));
    assert!(diags.iter().any(|d| d.message.contains("Missing mandatory block: 'TransitionInfo'")));
}

#[test]
fn test_duplicate_blocks_error() {
    let input = "RouteInfo:\nTransitionInfo:\nRouteInfo:"; 
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    assert_eq!(diags.len(), 1, "Should have exactly 1 error for the duplicate block");
    
    let error = &diags[0];
    assert_eq!(error.severity, Some(DiagnosticSeverity::ERROR));
    assert!(error.message.contains("Duplicate definition"));
    assert!(error.message.contains("RouteInfo"));
}

#[test]
fn test_duplicate_and_missing_combined() {
    let input = "RouteInfo:\nRouteInfo:"; 
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    assert_eq!(diags.len(), 2);
    
    let has_dup = diags.iter().any(|d| d.message.contains("Duplicate definition"));
    let has_missing = diags.iter().any(|d| d.message.contains("Missing mandatory block"));
    
    assert!(has_dup && has_missing);
}
