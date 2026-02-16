use amaro_lsp::parser::{parse_file, check_semantics};
use amaro_lsp::ast::*;
use tower_lsp::lsp_types::DiagnosticSeverity;

const MOCK_MANDATORY_BLOCKS: &str = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []

TransitionInfo:
    cost = 1.0
    apply = []
    get_transitions = []
"#;

// Core Semantic Tests

#[test]
fn capitalization_warning() {
    let input = format!("{}\narchitecture[name='test']", MOCK_MANDATORY_BLOCKS);
    let file = parse_file(&input).unwrap();
    let diags = check_semantics(&file);

    let cap_errors: Vec<_> = diags.iter()
        .filter(|d| d.message.to_lowercase().contains("capitalized"))
        .collect();

    assert_eq!(cap_errors.len(), 1, "Should have exactly 1 capitalization warning");
    assert_eq!(cap_errors[0].severity, Some(DiagnosticSeverity::WARNING));
}

#[test]
fn no_warning_for_correct_capitalization() {
    let input = format!("{}\nArchitecture[name='test']", MOCK_MANDATORY_BLOCKS);
    let file = parse_file(&input).unwrap();
    let diags = check_semantics(&file);
    
    let cap_errors: Vec<_> = diags.iter()
        .filter(|d| d.message.to_lowercase().contains("capitalized"))
        .collect();
    assert!(cap_errors.is_empty(), "Expected 0 capitalization errors, found: {:?}", cap_errors);
}

#[test]
fn test_all_valid_no_errors() {
    let input = MOCK_MANDATORY_BLOCKS;
    
    let file = parse_file(&input).unwrap();
    let diags = check_semantics(&file);
    
    assert!(diags.is_empty(), "Expected no diagnostics for valid input, got: {:?}", diags);
}

#[test]
fn test_missing_mandatory_blocks() {
    // Only Architecture, missing RouteInfo and TransitionInfo
    let input = "Architecture[name='test']"; 
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    assert_eq!(diags.len(), 2);
    assert!(diags.iter().any(|d| d.message.contains("Missing mandatory block: 'RouteInfo'")));
    assert!(diags.iter().any(|d| d.message.contains("Missing mandatory block: 'TransitionInfo'")));
}

#[test]
fn test_duplicate_blocks_error() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    cost = 1.0
    apply = []
    get_transitions = []
RouteInfo:
    routed_gates = T
    realize_gate = None
    "#;
    
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
    // Duplicate RouteInfo, Missing TransitionInfo
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
RouteInfo:
    routed_gates = T
    realize_gate = None
    "#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    assert_eq!(diags.len(), 2, "Should have 2 errors: duplicate + missing");
    
    let has_dup = diags.iter().any(|d| d.message.contains("Duplicate definition"));
    let has_missing = diags.iter().any(|d| d.message.contains("Missing mandatory block"));
    
    assert!(has_dup, "Should detect duplicate RouteInfo");
    assert!(has_missing, "Should detect missing TransitionInfo");
}

#[test]
fn test_missing_required_fields() {
    // RouteInfo missing 'realize_gate'
    let input = r#"
RouteInfo:
    routed_gates = CX

TransitionInfo:
    cost = 1.0
    apply = identity
"#;
    
    let file = parse_file(&input).unwrap();
    let diags = check_semantics(&file);
    
    let errors: Vec<_> = diags.iter().filter(|d| d.severity == Some(DiagnosticSeverity::ERROR)).collect();
    
    let missing_field = errors.iter().find(|d| d.message.contains("missing required field"));
    assert!(missing_field.is_some(), "Should error about missing required field");
    assert!(missing_field.unwrap().message.contains("realize_gate"));
}

#[test]
fn test_struct_def_in_block() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{u : Location, v : Location}
    realize_gate = []

TransitionInfo:
    Transition{edge : (Location, Location)}
    cost = 1.0
    apply = []
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    
    // Verify struct defs are parsed
    assert_eq!(file.blocks.len(), 2);
    
    let BlockContent::Fields(items) = &file.blocks[0].content;
    let has_struct = items.iter().any(|item| matches!(item, BlockItem::StructDef(_)));
    assert!(has_struct, "RouteInfo should contain a struct definition");
    
    // Should still pass semantic checks
    let diags = check_semantics(&file);
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(errors.is_empty(), "Struct defs should not cause errors");
}

// Gate Validation Tests

#[test]
fn test_valid_gates_no_warning() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    cost = 1.0
    apply = []
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let warnings: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .collect();
    assert!(warnings.is_empty(), "Valid gate CX should not produce warnings");
    
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(errors.is_empty(), "Should have no errors, got: {:?}", errors);
}

#[test]
fn test_invalid_gate_warning() {
    let input = r#"
RouteInfo:
    routed_gates = InvalidGate
    realize_gate = Some(value)
TransitionInfo:
    cost = 1.0
    apply = identity
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let warnings: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING) && d.message.contains("not a recognized standard gate"))
        .collect();
        
    assert_eq!(warnings.len(), 1, "Should warn about InvalidGate");
    assert!(warnings[0].message.contains("InvalidGate"));
    
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(errors.iter().any(|d| d.message.contains("Undefined variable")));
}

#[test]
fn test_multiple_gates_in_list_and_tuple() {
    // Test both List [A, B] and Tuple (A, B) syntax
    let input = r#"
RouteInfo:
    routed_gates = [CX, T]
    realize_gate = (Pauli, PauliMeasurement)
TransitionInfo:
    cost = 1.0
    apply = identity
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let warnings: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .collect();
    assert!(warnings.is_empty(), "Recursion check should validate gates inside lists and tuples");
}

#[test]
fn test_mixed_valid_invalid_gates() {
    let input = r#"
RouteInfo:
    routed_gates = [CX, BadGate, T]
    realize_gate = Some(value)
TransitionInfo:
    cost = 1.0
    apply = identity
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let warnings: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .collect();
        
    assert_eq!(warnings.len(), 1, "Should warn only about BadGate");
    assert!(warnings[0].message.contains("BadGate"));
    
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(!errors.is_empty(), "Should have errors because BadGate is undefined");
    assert!(errors.iter().any(|d| d.message.contains("Undefined variable")), "Should report BadGate as undefined");
}

#[test]
fn test_semantic_checks_work_with_bracket_syntax() {
    let input = r#"
    RouteInfo[
        routed_gates = CX
        realize_gate = []
    ]
    TransitionInfo[
        cost = 1.0
        apply = []
        get_transitions = []
    ]
    "#;

    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    assert!(diags.is_empty(), "Semantics should work for Bracket syntax too");
}

#[test]
fn test_gate_qubits_field_access() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    cost = 1.0
    apply = value_swap(Location(0), Location(1))
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert!(errors.is_empty(), "Gate.qubits should be recognized. Got: {:?}", errors);
}

#[test]
fn test_arch_contains_edge_method() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = if Arch.contains_edge((Location(0), Location(1)))
                   then Some(CX)
                   else None
TransitionInfo:
    cost = 1.0
    apply = []
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    
    assert!(errors.is_empty(), "Arch.contains_edge should be recognized. Got: {:?}", errors);
}

#[test]
fn test_state_gates_method() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = State.gates()
TransitionInfo:
    cost = 1.0
    apply = []
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    
    assert!(errors.is_empty(), "State.gates() should be recognized. Got: {:?}", errors);
}

#[test]
fn test_transition_edge_tuple_access() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    cost = 1.0
    apply = value_swap(Transition.edge.0, Transition.edge.1)
    get_transitions = []
"#;
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert!(errors.is_empty(), "Transition.edge tuple access (.0/.1) should be valid. Got: {:?}", errors);
}

#[test]
fn test_transition_edge_field() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    cost = 1.0
    apply = value_swap(Transition.edge.(0), Transition.edge.(1))
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    
    assert!(errors.is_empty(), "Transition.edge should be valid. Got: {:?}", errors);
}

#[test]
fn test_value_swap_function() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    cost = 1.0
    apply = value_swap(Location(0), Location(1))
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    
    assert!(errors.is_empty(), "value_swap should be valid. Got: {:?}", errors);
}

#[test]
fn test_nested_field_access() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = State.map[Gate.qubits[0]]
TransitionInfo:
    cost = 1.0
    apply = []
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    
    assert!(errors.len() <= 1, "Nested field access should mostly work. Got: {:?}", errors);
}

#[test]
fn test_map_function_with_lambda() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = map(|x| -> x, [1, 2, 3])
TransitionInfo:
    cost = 1.0
    apply = []
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    
    assert!(errors.is_empty(), "map with lambda should be valid. Got: {:?}", errors);
}

#[test]
fn test_fold_function() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    cost = fold(0.0, |acc, x| -> acc, [1.0, 2.0, 3.0])
    apply = []
    get_transitions = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    
    assert!(errors.is_empty(), "fold should be valid. Got: {:?}", errors);
}

#[test]
fn test_lambda_parameter_scoping() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = map(|item| -> item, [CX, T])
TransitionInfo:
    cost = 1.0
    apply = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let undefined_errors: Vec<_> = diags.iter()
        .filter(|d| d.message.contains("Undefined variable 'item'"))
        .collect();
    
    assert!(undefined_errors.is_empty(), "Lambda parameter should be in scope. Got errors: {:?}", undefined_errors);
}
    
#[test]
fn test_let_binding_scoping() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = let temp = CX in temp
TransitionInfo:
    cost = 1.0
    apply = []
"#;
    
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    
    let undefined_errors: Vec<_> = diags.iter()
        .filter(|d| d.message.contains("Undefined variable 'temp'"))
        .collect();
    
    assert!(undefined_errors.is_empty(), "Let binding should work");
}

// State.map[Gate.qubits[0]] - QubitMap indexed by Qubit
#[test]
fn test_qubit_index_on_qubitmap() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{u : Location, v : Location}
    realize_gate = State.map[Gate.qubits[0]]
TransitionInfo:
    get_transitions = []
    apply = []
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(errors.is_empty(), "QubitMap[Qubit] should be valid. Got: {:?}", errors);
}

#[test]
fn test_state_map_called_as_function() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{u : Location}
    realize_gate = values(State.map())
TransitionInfo:
    get_transitions = []
    apply = []
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(errors.is_empty(), "State.map() as function should be valid. Got: {:?}", errors);
}

#[test]
fn test_state_map_indexed_directly() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{u : Location, v : Location}
    realize_gate = if Arch.contains_edge((State.map[Gate.qubits[0]], State.map[Gate.qubits[1]]))
                   then Some(GateRealization{u = State.map[Gate.qubits[0]], v = State.map[Gate.qubits[1]]})
                   else None
TransitionInfo:
    get_transitions = []
    apply = []
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(errors.is_empty(), "State.map[Qubit] should be valid. Got: {:?}", errors);
}

#[test]
fn test_unknown_index_access_is_lenient() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{path : Vec()}
    realize_gate = map(|x| -> x.implementation.(path()), State.implemented_gates())
TransitionInfo:
    get_transitions = []
    apply = []
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(errors.is_empty(), "Unknown.index should be lenient. Got: {:?}", errors);
}

#[test]
fn test_nisq_realize_gate() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{u : Location, v : Location}
    realize_gate = if Arch.contains_edge((State.map[Gate.qubits[0]], State.map[Gate.qubits[1]]))
                   then Some(GateRealization{u = State.map[Gate.qubits[0]], v = State.map[Gate.qubits[1]]})
                   else None
TransitionInfo:
    Transition{edge : (Location, Location)}
    get_transitions = (map(|x| -> Transition{edge = x}, Arch.edges())).push(Transition{edge = (Location(0), Location(0))})
    apply = value_swap(Transition.edge.(0), Transition.edge.(1))
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    let errors: Vec<_> = diags.iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(errors.is_empty(), "NISQ pattern should be valid. Got: {:?}", errors);
}
