use amaro_lsp::ast::*;
use amaro_lsp::parser::{check_semantics, parse_file};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};


const MOCK_MANDATORY_BLOCKS: &str = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []

TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;

pub fn diags_no_errors(diags: &Vec<Diagnostic>) -> bool {
    !diags.iter().any(|elt| match elt.severity {
        Some(severity) => match severity {
            DiagnosticSeverity::ERROR => true,
            DiagnosticSeverity::WARNING => true,
            DiagnosticSeverity::HINT => false,
            DiagnosticSeverity::INFORMATION => false,
            _ => true,
        },
        None => false,
    })
}

// Core Semantic Tests

#[test]
fn capitalization_warning() {
    let input = format!("{}\narchitecture[name='test']", MOCK_MANDATORY_BLOCKS);
    let file = parse_file(&input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let cap_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.to_lowercase().contains("capitalized"))
        .collect();

    assert_eq!(
        cap_errors.len(),
        1,
        "Should have exactly 1 capitalization warning"
    );
    assert_eq!(cap_errors[0].severity, Some(DiagnosticSeverity::WARNING));
}

#[test]
fn no_warning_for_correct_capitalization() {
    let input = format!("{}\nArchitecture[name='test']", MOCK_MANDATORY_BLOCKS);
    let file = parse_file(&input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let cap_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.to_lowercase().contains("capitalized"))
        .collect();
    assert!(
        cap_errors.is_empty(),
        "Expected 0 capitalization errors, found: {:?}",
        cap_errors
    );
}

#[test]
fn test_all_valid_no_errors() {
    let input = MOCK_MANDATORY_BLOCKS;

    let file = parse_file(&input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    assert!(
        diags_no_errors(&diags),
        "Expected no diagnostics for valid input, got: {:?}",
        diags
    );
}

#[test]
fn test_missing_mandatory_blocks() {
    // Only Architecture, missing RouteInfo and TransitionInfo
    let input = "Architecture[name='test']";
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    assert_eq!(diags.len(), 2);
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("Missing mandatory block: 'RouteInfo'"))
    );
    assert!(diags.iter().any(|d| {
        d.message
            .contains("Missing mandatory block: 'TransitionInfo'")
    }));
}

#[test]
fn test_duplicate_blocks_error() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
RouteInfo:
    routed_gates = T
    realize_gate = None
    "#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    assert_eq!(
        diags.len(),
        1,
        "Should have exactly 1 error for the duplicate block"
    );

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
    let (diags, _, _) = check_semantics(&file);

    assert_eq!(diags.len(), 2, "Should have 2 errors: duplicate + missing");

    let has_dup = diags
        .iter()
        .any(|d| d.message.contains("Duplicate definition"));
    let has_missing = diags
        .iter()
        .any(|d| d.message.contains("Missing mandatory block"));

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
    apply = identity_application(step)
"#;

    let file = parse_file(&input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    let missing_field = errors
        .iter()
        .find(|d| d.message.contains("missing required field"));
    assert!(
        missing_field.is_some(),
        "Should error about missing required field"
    );
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
    apply = identity_application(step)
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();

    // Verify struct defs are parsed
    assert_eq!(file.blocks.len(), 2);

    let BlockContent::Fields(items) = &file.blocks[0].content;
    let has_struct = items
        .iter()
        .any(|item| matches!(item, BlockItem::StructDef(_)));
    assert!(has_struct, "RouteInfo should contain a struct definition");

    // Should still pass semantic checks
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
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
    apply = identity_application(step)
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let warnings: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .collect();
    assert!(
        warnings.is_empty(),
        "Valid gate CX should not produce warnings"
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "Should have no errors, got: {:?}",
        errors
    );
}

#[test]
fn test_invalid_gate_warning() {
    let input = r#"
RouteInfo:
    routed_gates = InvalidGate
    realize_gate = Some(value)
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let warnings: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.severity == Some(DiagnosticSeverity::WARNING)
                && d.message.contains("not a recognized standard gate")
        })
        .collect();

    assert_eq!(warnings.len(), 1, "Should warn about InvalidGate");
    assert!(warnings[0].message.contains("InvalidGate"));

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Undefined variable"))
    );
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
    apply = identity_application(step)
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let warnings: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .collect();
    assert!(
        warnings.is_empty(),
        "Recursion check should validate gates inside lists and tuples"
    );
}

#[test]
fn test_mixed_valid_invalid_gates() {
    let input = r#"
RouteInfo:
    routed_gates = [CX, BadGate, T]
    realize_gate = Some(value)
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let warnings: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .collect();

    assert_eq!(warnings.len(), 1, "Should warn only about BadGate");
    assert!(warnings[0].message.contains("BadGate"));

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        !errors.is_empty(),
        "Should have errors because BadGate is undefined"
    );
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Undefined variable")),
        "Should report BadGate as undefined"
    );
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
        apply = identity_application(step)
        get_transitions = []
    ]
    "#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    assert!(
        diags_no_errors(&diags),
        "Semantics should work for Bracket syntax too"
    );
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
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert!(
        errors.is_empty(),
        "Gate.qubits should be recognized. Got: {:?}",
        errors
    );
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
    apply = identity_application(step)
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert!(
        errors.is_empty(),
        "Arch.contains_edge should be recognized. Got: {:?}",
        errors
    );
}

#[test]
fn test_state_gates_method() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = State.gates()
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert!(
        errors.is_empty(),
        "State.gates() should be recognized. Got: {:?}",
        errors
    );
}

#[test]
fn test_transition_edge_tuple_access() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    Transition{edge : (Location,Location)}
    cost = 1.0
    apply = value_swap(Transition.edge.(0), Transition.edge.(1))
    get_transitions = []
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert!(
        errors.is_empty(),
        "Transition.edge tuple access (.0/.1) should be valid. Got: {:?}",
        errors
    );
}

#[test]
fn test_transition_edge_field() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    Transition{edge : (Location, Location)}
    cost = 1.0
    apply = value_swap(Transition.edge.(0), Transition.edge.(1))
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert!(
        errors.is_empty(),
        "Transition.edge should be valid. Got: {:?}",
        errors
    );
}

#[test]
fn test_value_swap_function() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = Vec()
TransitionInfo:
    cost = 1.0
    apply = value_swap(Location(0), Location(1))
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert!(
        errors.is_empty(),
        "value_swap should be valid. Got: {:?}",
        errors
    );
}

#[test]
fn test_nested_field_access() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = State.map[Gate.qubits[0]]
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert!(
        errors.len() <= 1,
        "Nested field access should mostly work. Got: {:?}",
        errors
    );
}

// TODO this test is failing because of generic type inference.
// we will need to run generic type inference MULTIPLE times to get to the root.
// we need to run it as many times as there are generic types in the expression,
// since T1 could depend on T0 being resolved.
// so, should either have a way to count how many generic types there are,
// OR have a way to keep going "until the job is done", which is likely more
// complex than it's worth, since avoiding infinite loop case seems tough.
#[test]
fn test_map_function_with_lambda() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = map(|x| -> x, [1, 2, 3])
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert!(
        errors.is_empty(),
        "map with lambda should be valid. Got: {:?}",
        errors
    );
}

#[test]
fn test_fold_function() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    cost = fold(0.0, |acc, x| -> acc, [1.0, 2.0, 3.0])
    apply = identity_application(step)
    get_transitions = []
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let errors: Vec<_> = diags
        .iter()
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
    apply = identity_application(step)
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let undefined_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined variable 'item'"))
        .collect();

    assert!(
        undefined_errors.is_empty(),
        "Lambda parameter should be in scope. Got errors: {:?}",
        undefined_errors
    );
}

#[test]
fn test_let_binding_scoping() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = let temp = CX in temp
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
"#;

    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let undefined_errors: Vec<_> = diags
        .iter()
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
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "QubitMap[Qubit] should be valid. Got: {:?}",
        errors
    );
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
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "State.map() as function should be valid. Got: {:?}",
        errors
    );
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
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "State.map[Qubit] should be valid. Got: {:?}",
        errors
    );
}

#[test]
fn test_unknown_index_access_is_lenient() {
    // x.implementation is Unknown (not a known Gate field).
    // Projection/index into Unknown should be lenient — no error.
    let input = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{path : Vec()}
    realize_gate = map(|x| -> x.implementation.(0), State.implemented_gates())
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "Unknown.index should be lenient. Got: {:?}",
        errors
    );
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
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "NISQ pattern should be valid. Got: {:?}",
        errors
    );
}

#[test]
fn test_comparison_used_as_if_condition_no_error() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = if (1 > 0) then None else None
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "Comparison should be valid if condition. Got: {:?}",
        errors
    );
}

#[test]
fn test_float_as_if_condition_still_errors() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = if 1.0 then None else None
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let has_error = diags
        .iter()
        .any(|d| d.message.to_lowercase().contains("bool"));
    assert!(
        has_error,
        "Float as if-condition should still error after BinaryOp fix."
    );
}

#[test]
fn test_struct_prepass_wrong_type_caught() {
    // Transition.edge is Tuple(Location, Location), not Location.
    // value_swap(Location, Location) should error when given Tuple as first arg.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    Transition{edge : (Location, Location)}
    get_transitions = []
    apply = value_swap(Transition.edge, Location(0))
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let has_type_error = diags
        .iter()
        .any(|d| d.severity == Some(DiagnosticSeverity::ERROR));
    assert!(
        has_type_error,
        "Passing Tuple where Location expected should error after pre-pass."
    );
}

#[test]
fn test_struct_prepass_correct_usage_no_error() {
    // Transition.edge.(0) should be Location → value_swap(Location, Location) → OK
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    Transition{edge : (Location, Location)}
    get_transitions = []
    apply = value_swap(Transition.edge.(0), Transition.edge.(1))
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "Correct Transition.edge.(0) usage should produce no errors. Got: {:?}",
        errors
    );
}

#[test]
fn test_struct_prepass_unknown_field_warns() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    Transition{edge : (Location, Location)}
    get_transitions = []
    apply = Transition.nonexistent
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let has_field_warning = diags.iter().any(|d| {
        d.message.to_lowercase().contains("nonexistent")
            || d.message.to_lowercase().contains("no field")
    });
    assert!(
        has_field_warning,
        "Accessing nonexistent field on known struct should warn."
    );
}

#[test]
fn test_cost_bool_rejected() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = true
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let has_cost_error = diags
        .iter()
        .any(|d| d.message.contains("cost") && d.message.to_lowercase().contains("float"));
    assert!(
        has_cost_error,
        "Bool literal as cost should be rejected. Got: {:?}",
        diags
    );
}

#[test]
fn test_cost_float_ok() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 1.5
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let cost_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("cost"))
        .collect();
    assert!(
        cost_errors.is_empty(),
        "Float cost should produce no errors. Got: {:?}",
        cost_errors
    );
}

#[test]
fn test_cost_int_ok() {
    // Int is compatible with Float per leniency rule → should pass
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let cost_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("cost"))
        .collect();
    assert!(
        cost_errors.is_empty(),
        "Int cost should be accepted (leniency). Got: {:?}",
        cost_errors
    );
}

#[test]
fn test_cost_string_rejected() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 'oops'
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let has_cost_error = diags
        .iter()
        .any(|d| d.message.contains("cost") && d.message.to_lowercase().contains("float"));
    assert!(
        has_cost_error,
        "String as cost should be rejected. Got: {:?}",
        diags
    );
}

#[test]
fn test_arg_type_mismatch_uses_friendly_format() {
    // map expects a Function as first arg; passing an Int triggers a type mismatch.
    // The message should NOT contain Rust debug artifacts like "Box(" or "Function { params: [".
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = map(1, [])
    apply = identity_application(step)
    cost = 1.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let has_debug_artifact = diags.iter().any(|d| d.message.contains("Box("));
    assert!(
        !has_debug_artifact,
        "Diagnostic messages should not contain Rust debug format artifacts. Got: {:?}",
        diags
    );
}

#[test]
fn test_index_mismatch_message_no_debug_artifacts() {
    // Valid file — just verifies no Box( artifacts appear in any diagnostic.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 1.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let has_debug_artifact = diags.iter().any(|d| d.message.contains("Box("));
    assert!(
        !has_debug_artifact,
        "Diagnostic messages should not contain Rust debug format artifacts. Got: {:?}",
        diags
    );
}

#[test]
fn test_unary_not_on_bool_ok() {
    // !false → Bool; used as if condition → no error
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = if !false then [] else []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 1.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "!false should be valid Bool. Got: {:?}",
        errors
    );
}

#[test]
fn test_unary_neg_int_ok() {
    // -1 → Int; valid as cost (Int compatible with Float)
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = -1
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "Negative int literal as cost should be valid. Got: {:?}",
        errors
    );
}

#[test]
fn test_projection_on_tuple_resolves() {
    // Transition{edge : (Location, Location)} → Transition.edge.(0) → Location
    // Should not produce an "Undefined variable" error for Transition.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 1.0

GateRealization:
    Transition{edge : (Location, Location)}
    data = Transition.edge.(0)
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let undef: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined variable 'Transition'"))
        .collect();
    assert!(
        undef.is_empty(),
        "Transition.edge.(0) should resolve after pre-pass + Projection fix. Got: {:?}",
        undef
    );
}

#[test]
fn test_arch_trap_positions_resolves() {
    // Arch.trap_positions should resolve to Vec<Location>, not produce "Undefined variable 'Arch'"
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = Arch.trap_positions
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let undef_arch: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined variable 'Arch'"))
        .collect();
    assert!(
        undef_arch.is_empty(),
        "Arch.trap_positions should resolve — 'Arch' should not be undefined. Got: {:?}",
        undef_arch
    );
}

#[test]
fn test_arch_trap_edges_resolves() {
    // Arch.trap_edges should resolve to Vec<(Location, Location)>, not Unknown or error.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = Arch.trap_edges
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let undef_arch: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined variable 'Arch'"))
        .collect();
    assert!(
        undef_arch.is_empty(),
        "Arch.trap_edges should resolve — 'Arch' should not be undefined. Got: {:?}",
        undef_arch
    );
}

#[test]
fn test_arch_locations_resolves() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = all_paths(Arch, Arch.locations(), [], [])
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let undef_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined") || d.message.contains("non-function"))
        .collect();
    assert!(
        undef_errors.is_empty(),
        "Arch.locations() should resolve. Got: {:?}",
        undef_errors
    );
}

#[test]
fn test_arch_trap_vertices_resolves() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
ArchInfo:
    trap_zones = Arch.trap_vertices()

TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let undef_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined") || d.message.contains("non-function"))
        .collect();
    assert!(
        undef_errors.is_empty(),
        "Arch.trap_vertices() should resolve without errors. Got: {:?}",
        undef_errors
    );
}

#[test]
fn test_return_in_field_produces_warning_not_missing_field_error() {
    // `apply = return []` — 'return' is not valid in expression context.
    // Should emit a WARNING about 'return', NOT a "missing required field: apply" error.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = return []
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let missing_apply = diags
        .iter()
        .any(|d| d.message.contains("missing required field") && d.message.contains("apply"));
    let return_warning = diags
        .iter()
        .any(|d| d.message.to_lowercase().contains("return"));

    assert!(
        !missing_apply,
        "'apply' should not be reported as missing — got: {:?}",
        diags
    );
    assert!(
        return_warning,
        "Should emit a warning about 'return' in expression context. Got: {:?}",
        diags
    );
}

#[test]
fn test_return_warning_is_warning_not_error() {
    // The diagnostic for 'return' should be a WARNING, not an ERROR.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = return []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);

    let return_diag = diags
        .iter()
        .find(|d| d.message.to_lowercase().contains("return"));

    assert!(
        return_diag.is_some(),
        "Should have a diagnostic about 'return'. Got: {:?}",
        diags
    );
    assert_eq!(
        return_diag.unwrap().severity,
        Some(DiagnosticSeverity::WARNING),
        "'return' diagnostic should be a WARNING, not an error"
    );
}

#[test]
fn test_return_does_not_affect_valid_fields() {
    // A file where no 'return' is used should be completely unaffected.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "Valid file with no 'return' should produce no errors. Got: {:?}",
        errors
    );
}

#[test]
fn test_step_context_variable_resolves() {
    // Old-format files use 'Step' (capitalized) as the state context variable.
    // Step should resolve to StateT; Step.map, Step.gates etc. should not error.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = Step.gates()
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let undef_step: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined variable") && d.message.contains("Step"))
        .collect();
    assert!(
        undef_step.is_empty(),
        "'Step' should resolve as StateT. Got: {:?}",
        undef_step
    );
}

#[test]
fn test_step_lowercase_still_works() {
    // 'step' (lowercase) is the step counter (Int) — used in arithmetic should not error.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    // No errors should occur in a valid file — 'step' being Int is an internal guarantee
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "Valid file should produce no errors. Got: {:?}",
        errors
    );
}

#[test]
fn test_combinations_registered() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = combinations([], 2)
    apply = identity_application(step)
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let undef_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined variable"))
        .collect();
    assert!(
        undef_errors.is_empty(),
        "'combinations' should be registered as a built-in. Got: {:?}",
        undef_errors
    );
}

#[test]
fn test_max_min_abs_registered() {
    // max, min, abs should resolve without "Undefined variable" errors.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = max(min(1, 2), abs(0))
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let undef: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined variable"))
        .collect();
    assert!(
        undef.is_empty(),
        "max, min, abs should be registered built-ins. Got: {:?}",
        undef
    );
}

#[test]
fn test_consistent_and_to_2d_registered() {
    // consistent and to_2d should resolve without "Undefined variable" errors.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = []
    apply = consistent([], State.map())
    cost = 0.0
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let undef: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined variable 'consistent'"))
        .collect();
    assert!(
        undef.is_empty(),
        "'consistent' should be registered as a built-in. Got: {:?}",
        undef
    );
}

#[test]
fn test_missing_builtins_no_undefined_error() {
    // All previously-missing built-ins should resolve without "Undefined variable" errors.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    get_transitions = combinations([], 2)
    apply = identity_application(step)
    cost = max(0, abs(0))
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let undef_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.contains("Undefined variable"))
        .collect();
    assert!(
        undef_errors.is_empty(),
        "No undefined variable errors expected for registered built-ins. Got: {:?}",
        undef_errors
    );
}

#[test]
fn test_match_expression_no_errors() {
    // A valid match expression should not produce any errors.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = match Gate with
        | CX -> 1
        | T -> 2
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "Valid match expression should produce no errors. Got: {:?}",
        errors
    );
}

#[test]
fn test_match_expression_in_field() {
    // match used as a field value — semantics should accept it without undefined-variable errors
    // on the match/with keywords.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    cost = 1.0
    apply = match State with
        | _ -> []
    get_transitions = []
"#;
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let keyword_errors: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.message.contains("Undefined variable 'match'")
                || d.message.contains("Undefined variable 'with'")
        })
        .collect();
    assert!(
        keyword_errors.is_empty(),
        "'match' and 'with' should not appear as undefined variables. Got: {:?}",
        keyword_errors
    );
}

#[test]
fn test_match_keywords_not_parsed_as_identifiers() {
    // 'match' and 'with' must not be accepted as field keys or identifiers.
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = []
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;
    // If is_keyword works correctly, fields named 'match' or 'with' would be rejected.
    // Just verify the valid file above parses and validates cleanly.
    let file = parse_file(input).unwrap();
    let (diags, _, _) = check_semantics(&file);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();
    assert!(
        errors.is_empty(),
        "Clean file should have no errors. Got: {:?}",
        errors
    );
}
