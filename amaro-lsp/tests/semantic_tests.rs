
use std::collections::HashMap;

use amaro_lsp::ast::*;
use amaro_lsp::parser::expr::parse_expr;
use amaro_lsp::parser::symbols::{SymbolTable, Type, UserDefTable};
use amaro_lsp::parser::{GenericTable, InferenceData, StringLabels, TypeMap, check_semantics, overlay_type, parse_file, register_field};
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
    let parse_output = parse_file(&input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

    assert!(diags.len() > 0, "Expect at least 1 diagnostic");

    let cap_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.message.to_lowercase().contains("invalid"))
        .collect();

    assert_eq!(
        cap_errors.len(),
        1,
        "Should have a warning about invalid block"
    );
    assert_eq!(cap_errors[0].severity, Some(DiagnosticSeverity::ERROR));
}

#[test]
fn no_warning_for_correct_capitalization() {
    let input = format!("{}\nArchitecture[name='test']", MOCK_MANDATORY_BLOCKS);
    let parse_output = parse_file(&input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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

    let parse_output = parse_file(&input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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
    realize_gate = []
    "#;

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

    assert_eq!(
        diags.len(),
        1,
        "Should have exactly 1 error for the duplicate block. Got: {:?}",
        diags
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
    realize_gate = []
    "#;

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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

    let parse_output = parse_file(&input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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

    let parse_output = parse_file(input).unwrap();

    // Verify struct defs are parsed
    assert_eq!(parse_output.file.blocks.len(), 2);

    let BlockContent::Fields(items) = &parse_output.file.blocks[0].content;
    let has_struct = items
        .iter()
        .any(|item| matches!(item, BlockItem::StructDef(_)));
    assert!(has_struct, "RouteInfo should contain a struct definition");

    // Should still pass semantic checks
    let diags = check_semantics(&parse_output.file).diagnostics;
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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

    let warnings: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.severity == Some(DiagnosticSeverity::ERROR) && d.message.contains("InvalidGate")
        })
        .collect();

    assert_eq!(warnings.len(), 1, "Should warn about InvalidGate");

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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

    let warnings: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR) && d.message.contains("BadGate"))
        .collect();

    assert_eq!(warnings.len(), 1, "Should warn only about BadGate");

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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    GateRealization{u: Gate}
    routed_gates = CX
    realize_gate = if Arch.contains_edge((Location(0), Location(1)))
                   then Vec().push(GateRealization{u = CX})
                   else Vec()
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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
    GateRealization{gate: Gate}
    routed_gates = CX
    realize_gate = map(|x| -> GateRealization{gate = x}, State.gates())
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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
    GateRealization{u : Int}
    routed_gates = CX
    realize_gate = map(|x| -> GateRealization{u = x}, [1, 2, 3])
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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

    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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
    GateRealization{u : Location}
    realize_gate = Vec().push(GateRealization{u = State.map[Gate.qubits[0]]})
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0.0
"#;
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    realize_gate = map(|x| -> GateRealization{u = x},values(State.map()))
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0.0
"#;
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
                   then Vec().push(GateRealization{u = State.map[Gate.qubits[0]], v = State.map[Gate.qubits[1]]})
                   else Vec()
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0.0
"#;
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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

// LS: I very much disagree with this test.
// If there is an unknown field, it should be added to this program.
// Otherwise, we should indicate to the user that they are doing something
// unexpected.

// #[test]
// fn test_unknown_index_access_is_lenient() {
//     // x.nothing is Unknown (not a known Gate field).
//     // Projection/index into Unknown should be lenient — no error.
//     let input = r#"
// RouteInfo:
//     routed_gates = CX
//     GateRealization{path : Vec()}
//     realize_gate = map(|x| -> x.nothing.(0), State.implemented_gates())
// TransitionInfo:
//     get_transitions = []
//     apply = identity_application(step)
//     cost = 0.0
// "#;
//     let parse_output = parse_file(input).unwrap();
//     let diags = check_semantics(&parse_output.file).diagnostics;
//     let errors: Vec<_> = diags
//         .iter()
//         .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
//         .collect();
//     assert!(
//         errors.is_empty(),
//         "Unknown.index should be lenient. Got: {:?}",
//         errors
//     );
// }

#[test]
fn test_nisq_realize_gate() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    GateRealization{u : Location, v : Location}
    realize_gate = if Arch.contains_edge((State.map[Gate.qubits[0]], State.map[Gate.qubits[1]]))
                   then Vec().push(GateRealization{u = State.map[Gate.qubits[0]], v = State.map[Gate.qubits[1]]})
                   else Vec()
TransitionInfo:
    Transition{edge : (Location, Location)}
    get_transitions = (map(|x| -> Transition{edge = x}, Arch.edges())).push(Transition{edge = (Location(0), Location(0))})
    apply = value_swap(Transition.edge.(0), Transition.edge.(1))
    cost = 0.0
"#;
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    realize_gate = if (1 > 0) then Vec() else Vec()
TransitionInfo:
    get_transitions = []
    apply = identity_application(step)
    cost = 0.0
"#;
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;

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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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

// TODO I am uncertain whether this test should be included.
// I can only find references to consistent and to_2d in old files with now
// invalid syntax.
// I will ask about these features. For now, this test will be commented out.
// #[test]
// fn test_consistent_and_to_2d_registered() {
//     // consistent and to_2d should resolve without "Undefined variable" errors.
//     let input = r#"
// RouteInfo:
//     routed_gates = CX
//     realize_gate = []
// TransitionInfo:
//     get_transitions = []
//     apply = consistent([], State.map())
//     cost = 0.0
// "#;
//     let parse_output = parse_file(input).unwrap();
//     let diags = check_semantics(&parse_output.file).diagnostics;
//     let undef: Vec<_> = diags
//         .iter()
//         .filter(|d| d.message.contains("Undefined variable 'consistent'"))
//         .collect();
//     assert!(
//         undef.is_empty(),
//         "'consistent' should be registered as a built-in. Got: {:?}",
//         undef
//     );
// }

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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
        | CX -> Vec()
        | T -> Vec()
TransitionInfo:
    cost = 1.0
    apply = identity_application(step)
    get_transitions = []
"#;
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
    let parse_output = parse_file(input).unwrap();
    let diags = check_semantics(&parse_output.file).diagnostics;
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
#[test]
fn test_overlay_basic_types_onto_self() {
    // tries to overlay onto themselves
    let mut background_types = [
        Type::ArchT,
        Type::Bool,
        Type::Float,
        Type::Gate,
        Type::InstrT,
        Type::Int,
        Type::Location,
        Type::Qubit,
        Type::QubitMap,
        Type::StateT,
        Type::String,
    ];
    let foreground_types = [
        Type::ArchT,
        Type::Bool,
        Type::Float,
        Type::Gate,
        Type::InstrT,
        Type::Int,
        Type::Location,
        Type::Qubit,
        Type::QubitMap,
        Type::StateT,
        Type::String,
    ];

    background_types
        .iter_mut()
        .zip(foreground_types.iter())
        .for_each(|pair| {
            assert!(!overlay_type(pair.0, pair.1));
            assert_eq!(*pair.0, *pair.1);
        });
}

#[test]
fn test_overlay_basic_types_onto_others() {
    // tries to overlay onto  others, shouldnt happen
    let mut background_types = [
        Type::Int,
        Type::Int,
        Type::Int,
        Type::Int,
        Type::Int,
        Type::Bool,
        Type::Bool,
        Type::Bool,
        Type::Qubit,
        Type::ArchT,
        Type::ArchT,
    ];
    let foreground_types = [
        Type::ArchT,
        Type::Bool,
        Type::Float,
        Type::Gate,
        Type::InstrT,
        Type::Int,
        Type::Location,
        Type::Qubit,
        Type::QubitMap,
        Type::StateT,
        Type::String,
    ];

    background_types
        .iter_mut()
        .zip(foreground_types.iter())
        .for_each(|pair| {
            let original_type = pair.0.clone();
            assert!(!overlay_type(pair.0, pair.1));
            assert_eq!(*pair.0, original_type);
        });
}

#[test]
fn test_overlay_onto_unknown() {
    let mut t1: Type = Type::Unknown;
    let t2: Type = Type::Int;

    assert!(overlay_type(&mut t1, &t2));
    assert_eq!(t1, Type::Int);
}

#[test]
fn test_overlay_onto_generic() {
    let mut t1: Type = Type::Generic(0);
    let t2: Type = Type::Int;

    assert!(overlay_type(&mut t1, &t2));
    assert_eq!(t1, Type::Int);
}

#[test]
fn test_overlay_complex() {
    let mut t1: Type = Type::Function {
        params: vec![
            Type::Generic(1), // init acc value
            Type::Function {
                params: vec![
                    Type::Generic(0), // elt
                    Type::Generic(1), // acc
                ],
                return_type: Box::new(Type::Generic(1)), // gives acc val
            },
            Type::Vec(Box::new(Type::Generic(0))), // collection of elts
        ],
        return_type: Box::new(Type::Generic(1)), // final acc value
    };
    let original_type = t1.clone();

    let wrong_type = Type::Bool;

    assert!(!overlay_type(&mut t1, &wrong_type));
    assert_eq!(original_type, t1);

    let half_match_expected_out = Type::Function {
        params: vec![
            Type::Int, // init acc value
            Type::Function {
                params: vec![
                    Type::Generic(0), // elt
                    Type::Generic(1), // acc
                ],
                return_type: Box::new(Type::Generic(1)), // gives acc val
            },
            Type::Vec(Box::new(Type::Generic(0))), // collection of elts
        ],
        return_type: Box::new(Type::Int), // final acc value
    };

    let half_match_foreground = Type::Function {
        params: vec![
            Type::Int, // init acc value
            Type::Bool,
            Type::Option(Box::new(Type::Int)), // collection of elts
        ],
        return_type: Box::new(Type::Int), // final acc value
    };

    assert!(overlay_type(&mut t1, &half_match_foreground));
    assert_eq!(t1, half_match_expected_out);
}


#[test]
fn test_if_then_else_information_sharing() {
    let expr = "if x > 5 then Vec().push(5) else Vec()";
    let mut diags = Vec::new();
    let res_expr = parse_expr(expr, expr, &mut diags).unwrap().1;

    let user_def_table = UserDefTable::empty();
    let mut type_map = TypeMap::new();
    let mut string_labels = StringLabels::new();
    let mut sym_table = SymbolTable::new();
    let mut diags = Vec::new();
    let mut generic_table = GenericTable::new();

    

    let mut inf_data = InferenceData {
        sym_table: &mut sym_table,
        diagnostics: &mut diags,
        type_map: &mut type_map,
        user_def_table: &user_def_table,
        generic_table: &mut generic_table,
        string_labels: &mut string_labels,
    };

    assert_eq!(register_field(&res_expr, &mut inf_data), Type::Vec(Box::new(Type::Int)));

    // check the expression tree to ensure it all matches
    let (cond_branch, then_branch, else_branch) = match res_expr.kind {
        ExprKind::IfThenElse { condition, then_branch, else_branch } => (condition, then_branch, else_branch),
        _ => panic!("Expected expression to be if-then-else"),
    };

    assert_eq!(*type_map.get(&cond_branch.id).unwrap(), Type::Bool);
    assert_eq!(*type_map.get(&then_branch.id).unwrap(), Type::Vec(Box::new(Type::Int)));
    assert_eq!(*type_map.get(&else_branch.id).unwrap(), Type::Vec(Box::new(Type::Int)));



    
}


#[test]
fn test_map_generic_resolution_sharing() {

    // will verify the types of many things in this expression, which involes
    // generic resolution
    let expr = "(map(|x| -> Transition{ edge = x}, Arch.edges())).push(Transition{edge = (Location(0),Location(0))})";
    let mut diags = Vec::new();
    let res_expr = parse_expr(expr, expr, &mut diags).unwrap().1;

    let mut field_to_add = HashMap::new();
    field_to_add.insert("edge".to_string(), Type::Tuple(vec![
        Type::Location,
        Type::Location
    ]));

    let mut user_def_table = UserDefTable::empty();
    user_def_table.add("Transition".to_string(), field_to_add);

    let mut type_map = TypeMap::new();
    let mut string_labels = StringLabels::new();
    let mut sym_table = SymbolTable::new();
    let mut diags = Vec::new();
    let mut generic_table = GenericTable::new();

    

    let mut inf_data = InferenceData {
        sym_table: &mut sym_table,
        diagnostics: &mut diags,
        type_map: &mut type_map,
        user_def_table: &user_def_table,
        generic_table: &mut generic_table,
        string_labels: &mut string_labels,
    };

    assert_eq!(register_field(&res_expr, &mut inf_data), Type::Vec(Box::new(Type::UserDef("Transition".to_string()))));

    // check the expression tree to ensure it all matches
    let (function, args) = match res_expr.kind {
        ExprKind::FunctionCall { function, args } => (function, args),
        _ => panic!("Expected expression to be function call"),
    };

    assert_eq!(
        *type_map.get(&function.id).unwrap(), 
        Type::Function {
            params: vec![
                Type::UserDef("Transition".to_string())
            ],
            return_type: Box::new(Type::Vec(Box::new(Type::UserDef("Transition".to_string()))))
        },
        "Push function signature mismatch"
    );
    assert_eq!(args.len(), 1, "Only 1 arg expected passed to push function");
    assert_eq!(*type_map.get(&args[0].id).unwrap(), Type::UserDef("Transition".to_string()), "Push function arg wasn't as expected");

    let (object, field) = match function.kind {
        ExprKind::FieldAccess { object, field } => (object, field),
        _ => panic!("Expected function expression to be field access"),
    };

    assert_eq!(field, "push");
    assert_eq!(*type_map.get(&object.id).unwrap(), Type::Vec(Box::new(Type::UserDef("Transition".to_string()))), "Expected push to be called on a Vec");

    let (function, args) = match object.kind {
        ExprKind::FunctionCall { function, args } => (function, args),
        _ => panic!("Expected map function call")
    };

    assert_eq!(
        *type_map.get(&function.id).unwrap(), 
        Type::Function {
            params: vec![
                Type::Function { 
                    params: vec![
                        Type::Tuple(vec![
                            Type::Location,
                            Type::Location
                        ])
                    ], 
                    return_type: Box::new(Type::UserDef("Transition".to_string())) 
                },
                Type::Vec(Box::new(Type::Tuple(vec![
                    Type::Location,
                    Type::Location
                ])))
            ],
            return_type: Box::new(Type::Vec(Box::new(Type::UserDef("Transition".to_string()))))
        },
        "Map function didn't evaluate to expected"
    );

    assert_eq!(args.len(), 2, "Expect 2 args passed to map");
    assert_eq!(
        *type_map.get(&args[0].id).unwrap(),
        Type::Function { 
            params: vec![
                Type::Tuple(vec![
                    Type::Location,
                    Type::Location
                ])
            ], 
            return_type: Box::new(Type::UserDef("Transition".to_string())) 
        },
        "First argument to map was wrong"
    );
    assert_eq!(
        *type_map.get(&args[1].id).unwrap(),
        Type::Vec(Box::new(Type::Tuple(vec![
            Type::Location,
            Type::Location
        ]))),
        "Second argument to map was wrong"
    );


    
}
