use amaro_lsp::parser::{parse_file, check_semantics};
use tower_lsp::lsp_types::DiagnosticSeverity;

const MOCK_MANDATORY_BLOCKS: &str = r#"
RouteInfo: {
    routed_gates = [];
    realize_gate = [];
}
TransitionInfo: {
    cost = [];
    apply = [];
}
"#;

#[test]
fn capitalization_warning() {
    let input = format!("{}{}", MOCK_MANDATORY_BLOCKS, "architecture[1]");
    let file = parse_file(&input).unwrap();
    let diags = check_semantics(&file);

    let cap_errors: Vec<_> = diags.iter()
        .filter(|d| d.message.contains("Capitalized"))
        .collect();

    assert_eq!(cap_errors.len(), 1, "Should have exactly 1 capitalization warning");
    assert_eq!(cap_errors[0].severity, Some(DiagnosticSeverity::WARNING));
}

#[test]
fn no_warning_for_correct_capitalization() {
    let input = format!("{}{}", MOCK_MANDATORY_BLOCKS, "Architecture[1]");
    let file = parse_file(&input).unwrap();
    let diags = check_semantics(&file);
    assert!(diags.is_empty(), "Expected 0 errors, found: {:?}", diags);
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
    let input = r#"
    RouteInfo: { routed_gates=[]; realize_gate=[]; }
    TransitionInfo: { cost=[]; apply=[]; }
    RouteInfo: { routed_gates=[]; realize_gate=[]; }
    "#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    assert_eq!(diags.len(), 1, "Should have exactly 1 error for the duplicate block");
    
    let error = &diags[0];
    println!("Error: {:?}", error);
    assert_eq!(error.severity, Some(DiagnosticSeverity::ERROR));
    assert!(error.message.contains("Duplicate definition"));
    assert!(error.message.contains("RouteInfo"));
}

#[test]
fn test_duplicate_and_missing_combined() {
    let input = r#"
    RouteInfo: { routed_gates=[]; realize_gate=[]; }
    RouteInfo: { routed_gates=[]; realize_gate=[]; }
    "#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    println!("Diagnostics: {:?}", diags);
    assert_eq!(diags.len(), 2);
    
    let has_dup = diags.iter().any(|d| d.message.contains("Duplicate definition"));
    let has_missing = diags.iter().any(|d| d.message.contains("Missing mandatory block"));
    
    assert!(has_dup, "Should detect duplicate RouteInfo");
    assert!(has_missing, "Should detect missing TransitionInfo");
}

#[test]
fn test_missing_required_fields() {
    let input = r#"RouteInfo:
    routed_gates = CX

TransitionInfo:
    cost = 1.0
    apply = func()"#;
    
    let file = parse_file(&input).unwrap();
    let diags = check_semantics(&file);
    
    let errors: Vec<_> = diags.iter().filter(|d| d.severity == Some(DiagnosticSeverity::ERROR)).collect();
    println!("Diagnostics: {:?}", diags);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("missing required field"));
    assert!(errors[0].message.contains("realize_gate"));
}

#[test]
fn test_all_valid_no_errors() {
    let input = format!("{}", MOCK_MANDATORY_BLOCKS);
    
    let file = parse_file(&input).unwrap();
    let diags = check_semantics(&file);
    
    assert!(diags.is_empty(), "Expected no diagnostics for valid input, got: {:?}", diags);
}
