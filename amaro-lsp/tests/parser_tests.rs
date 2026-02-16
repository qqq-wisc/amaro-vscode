use amaro_lsp::ast::*;
use amaro_lsp::parser::{
    consume_remaining_block, parse_file, parse_identifier, parse_rust_embedded_robust,
};

// Helper to extract first field's value expression from parsed file
fn get_first_field_value(file: AmaroFile) -> Expr {
    let block = &file.blocks[0];
    let BlockContent::Fields(items) = &block.content;
    if let Some(BlockItem::Field(field)) = items.first() {
        return field.value.clone();
    }
    panic!("Test Setup Error: Expected a block with at least one field");
}

// 1. Basic Parsing Tests (Identifiers & Rust Embedding)

#[test]
fn test_parse_identifier_valid() {
    assert!(parse_identifier("GateRealization").is_ok());
    assert!(parse_identifier("_internal").is_ok());
    assert!(parse_identifier("RouteInfo").is_ok());
}

#[test]
fn test_parse_rust_embedded_basic() {
    let input = r#"{{
fn test() {
    let x = 5;
}
}}"#;
    let result = parse_rust_embedded_robust(input);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().0, "");
}

#[test]
fn test_parse_rust_embedded_inline() {
    let input = r#"{{ fn foo() { let x = 1; } }}"#;
    assert!(parse_rust_embedded_robust(input).is_ok());
}

#[test]
fn test_parse_identifier_invalid() {
    assert!(parse_identifier("123Invalid").is_err());
    assert!(parse_identifier("").is_err());
    assert!(parse_identifier("{{").is_err());
}

// 2. Block Parsing Tests (Brackets & Colons)

#[test]
fn test_simple_bracket_block() {
    let input = r#"GateRealization[
    routed_gates = CX
    name = 'IonCNOT'
]"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 1);
    assert_eq!(file.blocks[0].kind, "GateRealization");
}

#[test]
fn test_nested_bracket_block() {
    let input = r#"GateRealization[
    data = (path : Vec<Location>)
    realize_gate = map(|x| -> GateRealization{path = x}, all_paths())
]"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 1);
}

#[test]
fn test_multiple_bracket_blocks() {
    let input = r#"GateRealization[
    name = 'test1'
]

Transition[
    name = 'test2'
]

Architecture[
    name = 'test3'
]"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 3);
    assert_eq!(file.blocks[0].kind, "GateRealization");
    assert_eq!(file.blocks[1].kind, "Transition");
    assert_eq!(file.blocks[2].kind, "Architecture");
}

#[test]
fn test_simple_colon_block() {
    let input = r#"RouteInfo:
    routed_gates = CX
    realize_gate = Some(value)"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 1);
    assert_eq!(file.blocks[0].kind, "RouteInfo");
}

#[test]
fn test_colon_block_with_structs() {
    let input = r#"RouteInfo:
    routed_gates = CX
    GateRealization{u : Location, v : Location}

TransitionInfo:
    Transition{edge : (Location,Location)}"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 2);
    assert_eq!(file.blocks[0].kind, "RouteInfo");
    assert_eq!(file.blocks[1].kind, "TransitionInfo");
}

#[test]
fn test_mixed_bracket_and_colon() {
    let input = r#"GateRealization[
    name = 'test'
]

RouteInfo:
    routed_gates = CX

Transition[
    cost = 1.0
]"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 3);
}

#[test]
fn test_consume_stops_at_next_block() {
    let input = r#"routed_gates = CX
data = test
GateRealization[
name = 'next'
]"#;

    let (rest, _) = consume_remaining_block(input).unwrap();
    assert!(rest.starts_with("GateRealization["));
}

#[test]
fn test_consecutive_colon_blocks() {
    let input = r#"RouteInfo:
    data = test

TransitionInfo:
    data = test

ArchInfo:
    width = 10

StateInfo:
    cost = 1.0"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 4);
}

#[test]
fn test_consume_stops_at_next_colon_block() {
    let input = r#"routed_gates = CX
data = test
TransitionInfo:
more data"#;

    let (rest, _) = consume_remaining_block(input).unwrap();
    assert!(rest.starts_with("TransitionInfo:"));
}

#[test]
fn test_consume_ignores_struct_definitions() {
    let input = r#"routed_gates = CX
GateRealization{u : Location}
Transition{edge : (Location,Location)}
realize_gate = Some(value)
RouteInfo:
next block"#;

    let (rest, _) = consume_remaining_block(input).unwrap();
    assert!(rest.starts_with("RouteInfo:"));
}

// 3. Expression Parsing Tests

#[test]
fn test_lambda_expression() {
    let input = r#"RouteInfo:
    realize_gate = map(|x| -> GateRealization{path = x}, all_paths())"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 1);

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert_eq!(field.key, "realize_gate");
        if let ExprKind::FunctionCall { args, .. } = &field.value.kind {
            assert_eq!(args.len(), 2);
            assert!(matches!(args[0].kind, ExprKind::Lambda { .. }));
        } else {
            panic!("Expected function call");
        }
    }
}

#[test]
fn test_if_then_else_expression() {
    let input = r#"TransitionInfo:
    cost = if x == y then 0.0 else 1.0"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert!(matches!(field.value.kind, ExprKind::IfThenElse { .. }));
    }
}

#[test]
fn test_let_binding_expression() {
    let input = r#"TransitionInfo:
    cost = let foo = 1.0 in foo"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert!(matches!(field.value.kind, ExprKind::LetBinding { .. }));
    }
}

#[test]
fn test_field_access() {
    let input = r#"RouteInfo:
    value = State.map[Gate.qubits[0]]"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert!(matches!(field.value.kind, ExprKind::IndexAccess { .. }));
    }
}

#[test]
fn test_tuple_projection() {
    let input = r#"TransitionInfo:
    value = Transition.edge.(0)"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert!(matches!(field.value.kind, ExprKind::Projection { .. }));
    }
}

#[test]
fn test_struct_literal() {
    let input = r#"RouteInfo:
    realize_gate = GateRealization{u = loc1, v = loc2}"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        if let ExprKind::StructLiteral { name, fields } = &field.value.kind {
            assert_eq!(name, "GateRealization");
            assert_eq!(fields.len(), 2);
        } else {
            panic!("Expected struct literal");
        }
    }
}

#[test]
fn test_comparison_operators() {
    let input = r#"TransitionInfo:
    cost = if edge == Location(0, 0) then 0.0 else 1.0"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        if let ExprKind::IfThenElse { condition, .. } = &field.value.kind {
            assert!(matches!(condition.kind, ExprKind::BinaryOp { .. }));
        }
    }
}

// 4. Edge Case & Recovery Tests

#[test]
fn test_file_with_rust_and_let_bindings() {
    let input = r#"{{
fn get_pair_cost(pair: (Location, Location)) -> f64 {
    let cost = 0.0;
    return cost;
}
}}

// Top-level let bindings are treated as ignored content by the loop
// but shouldn't crash parsing of subsequent blocks.

GateRealization[
    routed_gates = CX
]

Transition[
    name = 'IonTransition'
]

Architecture[
    name = 'IonArch'
]"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 3);
}

#[test]
fn test_empty_file() {
    let file = parse_file("").unwrap();
    assert_eq!(file.blocks.len(), 0);
}

#[test]
fn test_comments_in_file() {
    let input = r#"// comment
GateRealization[
    // comment
    name = 'test'
]"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 1);
}

#[test]
fn test_error_recovery_skips_invalid_lines() {
    let input = r#"this is invalid
still invalid

GateRealization[
    name = 'valid'
]

nonsense garbage

Transition[
    name = 'also valid'
]"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 2);
    assert_eq!(file.blocks[0].kind, "GateRealization");
    assert_eq!(file.blocks[1].kind, "Transition");
}

#[test]
fn test_all_rust_block_positions() {
    // Rust block at start
    let input1 = r#"{{ fn test() { qubit: Type } }}
GateRealization[name='test']"#;
    let result1 = parse_file(input1).unwrap();
    assert_eq!(result1.blocks.len(), 1);

    // Rust block between blocks
    let input2 = r#"GateRealization[name='a']
{{ fn test() { qubit: Type } }}
Transition[name='b']"#;
    let result2 = parse_file(input2).unwrap();
    assert_eq!(result2.blocks.len(), 2);
}

#[test]
fn test_whitespace_only() {
    let file = parse_file("   \n\n   ").unwrap();
    assert_eq!(file.blocks.len(), 0);
}

#[test]
fn test_windows_line_endings() {
    let input = "RouteInfo:\r\n  routed_gates = CX\r\n\r\nTransitionInfo:\r\n  data = test";
    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 2);
}

#[test]
fn test_file_no_trailing_newline() {
    let input = r#"GateRealization[
    name = 'test'
]"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 1);
}

#[test]
fn test_rust_block_is_ignored() {
    let input = r#"
GateRealization[name='test']
{{
qubit: *aod_qubit,
}}
"#;
    let file = parse_file(input).unwrap();
    // Should find GateRealization, but IGNORE the rust block contents
    assert_eq!(file.blocks.len(), 1);
    assert_eq!(file.blocks[0].kind, "GateRealization");
}

#[test]
fn test_rust_block_not_parsed_as_colon_block() {
    let input = r#"{{
fn test() {
    qubit: SomeType,
}
}}

GateRealization[
    name = 'test'
]"#;
    let result = parse_file(input);
    assert!(result.is_ok());
    let file = result.unwrap();
    assert_eq!(file.blocks.len(), 1);
    assert_eq!(file.blocks[0].kind, "GateRealization");
}

#[test]
fn test_node_ids_are_unique() {
    let input = r#"
RouteInfo:
    routed_gates = CX
    realize_gate = Some(value)

TransitionInfo:
    cost = 1.0
    apply = identity
"#;

    let file = parse_file(input).unwrap();

    let mut ids = std::collections::HashSet::new();

    ids.insert(file.id);
    for block in &file.blocks {
        ids.insert(block.id);
        let BlockContent::Fields(items) = &block.content;
        for item in items {
            match item {
                BlockItem::Field(field) => {
                    ids.insert(field.id);
                    ids.insert(field.value.id);
                }
                BlockItem::StructDef(struct_def) => {
                    ids.insert(struct_def.id);
                }
            }
        }
    }

    assert!(ids.len() >= 10, "Should have many unique node IDs");
}

// Multiline Expression Tests

#[test]
fn test_multiline_if_then_else() {
    let input = r#"RouteInfo:
    realize_gate = if condition
        then result1
        else result2"#;

    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 1);

    let BlockContent::Fields(items) = &file.blocks[0].content;
    assert_eq!(items.len(), 1);
    if let Some(BlockItem::Field(field)) = items.first() {
        assert_eq!(field.key, "realize_gate");
        assert!(matches!(field.value.kind, ExprKind::IfThenElse { .. }));
    } else {
        panic!("Expected field, got struct def");
    }
}

#[test]
fn test_multiline_if_with_complex_condition() {
    let input = r#"RouteInfo:
    realize_gate = if Arch.contains_edge((State.map[Gate.qubits[0]], State.map[Gate.qubits[1]]))
        then Some(GateRealization{u = State.map[Gate.qubits[0]]})
        else None"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert_eq!(field.key, "realize_gate");
        if let ExprKind::IfThenElse {
            condition,
            then_branch,
            else_branch,
        } = &field.value.kind
        {
            assert!(matches!(condition.kind, ExprKind::FunctionCall { .. }));
            assert!(matches!(then_branch.kind, ExprKind::Some(_)));
            assert!(matches!(else_branch.kind, ExprKind::None));
        } else {
            panic!("Expected if-then-else");
        }
    }
}

#[test]
fn test_multiline_let_binding() {
    let input = r#"TransitionInfo:
    cost = let foo = if x == y
            then 0.0
            else 1.0
        in foo"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert_eq!(field.key, "cost");
        assert!(matches!(field.value.kind, ExprKind::LetBinding { .. }));
    }
}

#[test]
fn test_multiline_lambda() {
    let input = r#"RouteInfo:
    realize_gate = map(|x|
        -> GateRealization{path = x}, 
        all_paths())"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert_eq!(field.key, "realize_gate");
        if let ExprKind::FunctionCall { args, .. } = &field.value.kind {
            assert!(matches!(args[0].kind, ExprKind::Lambda { .. }));
        }
    }
}

#[test]
fn test_deeply_nested_multiline_expression() {
    let input = r#"RouteInfo:
    realize_gate = if (Gate.gate_type()) == CX
        then
            map(|x| -> GateRealization{path = x},
                all_paths(arch,
                    vertical_neighbors(State.map[Gate.qubits[0]], Arch.width)))
        else
            map(|y| -> GateRealization{path = y},
                other_paths())"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    assert_eq!(items.len(), 1);
    if let Some(BlockItem::Field(field)) = items.first() {
        assert_eq!(field.key, "realize_gate");
        assert!(matches!(field.value.kind, ExprKind::IfThenElse { .. }));
    } else {
        panic!("Expected realize_gate field but got: {:?}", items);
    }
}

#[test]
fn test_multiline_with_comments() {
    let input = r#"RouteInfo:
    realize_gate = if condition  // Check condition
        then result1  // First option
        else result2  // Second option"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert_eq!(field.key, "realize_gate");
        assert!(matches!(field.value.kind, ExprKind::IfThenElse { .. }));
    }
}

#[test]
fn test_comma_separated_gates_no_brackets() {
    let input = r#"RouteInfo:
    routed_gates = CX, T
    realize_gate = Some(value)"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert_eq!(field.key, "routed_gates");
        // Should parse as a List even without brackets
        if let ExprKind::List(gates) = &field.value.kind {
            assert_eq!(gates.len(), 2);
            assert!(matches!(gates[0].kind, ExprKind::Identifier(ref s) if s == "CX"));
            assert!(matches!(gates[1].kind, ExprKind::Identifier(ref s) if s == "T"));
        } else {
            panic!("Expected List, got: {:?}", field.value.kind);
        }
    }
}

#[test]
fn test_comma_separated_vs_bracket_list_equivalence() {
    let input_no_brackets = r#"RouteInfo:
    routed_gates = CX, T, Pauli
    realize_gate = Some(value)"#;

    let input_brackets = r#"RouteInfo:
    routed_gates = [CX, T, Pauli]
    realize_gate = Some(value)"#;

    let file1 = parse_file(input_no_brackets).unwrap();
    let file2 = parse_file(input_brackets).unwrap();

    let BlockContent::Fields(items1) = &file1.blocks[0].content;
    let BlockContent::Fields(items2) = &file2.blocks[0].content;

    let field1 = items1.first().unwrap();
    let field2 = items2.first().unwrap();

    let BlockItem::Field(f1) = field1 else {
        panic!("Expected field")
    };
    let BlockItem::Field(f2) = field2 else {
        panic!("Expected field")
    };

    let ExprKind::List(gates1) = &f1.value.kind else {
        panic!("No brackets: Expected List, got: {:?}", f1.value.kind)
    };
    let ExprKind::List(gates2) = &f2.value.kind else {
        panic!("With brackets: Expected List, got: {:?}", f2.value.kind)
    };

    assert_eq!(gates1.len(), 3);
    assert_eq!(gates2.len(), 3);

    for (g1, g2) in gates1.iter().zip(gates2.iter()) {
        let ExprKind::Identifier(name1) = &g1.kind else {
            panic!("Expected identifier")
        };
        let ExprKind::Identifier(name2) = &g2.kind else {
            panic!("Expected identifier")
        };
        assert_eq!(name1, name2);
    }
}

#[test]
fn test_if_precedence_fix() {
    let input = r#"
RouteInfo:
    apply = if (Gate.gate_type) == CX then DoSomething else Skip
"#;

    let file = parse_file(input).expect("Should parse valid if-expression");
    let expr = get_first_field_value(file);

    if let ExprKind::IfThenElse { condition, .. } = expr.kind {
        match condition.kind {
            ExprKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Eq),
            _ => panic!(
                "FAILED: Parser stopped early! It didn't see the '==' inside the if-condition."
            ),
        }
    } else {
        panic!("Expected IfThenElse expression");
    }
}

#[test]
fn test_binary_op_multiline() {
    let input = r#"
RouteInfo:
    check = Gate.type 
            == 
            CX
"#;
    let file = parse_file(input).expect("Should handle newlines in binary ops");
    let expr = get_first_field_value(file);

    match expr.kind {
        ExprKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::Eq),
        _ => panic!("Expected BinaryOp eq"),
    }
}

#[test]
fn test_if_multiline_structure() {
    let input = r#"
RouteInfo:
    res = if Gate.is_valid
          then 
            Proceed
          else 
            Stop
"#;
    assert!(
        parse_file(input).is_ok(),
        "Failed to parse multiline If/Then/Else"
    );
}

#[test]
fn test_nested_logic_parens() {
    let input = r#"
RouteInfo:
    val = (A == B) && (C != D)
"#;
    let file = parse_file(input).expect("Should parse nested logic");
    let expr = get_first_field_value(file);

    match expr.kind {
        ExprKind::BinaryOp { op, .. } => assert_eq!(op, BinaryOperator::And),
        _ => panic!("Precedence logic failed. Top level should be &&"),
    }
}

#[test]
fn test_simple_dot_access() {
    let input = r#"
RouteInfo:
    val = State.map
"#;
    let file = parse_file(input).expect("Should parse dot access");
    let expr = get_first_field_value(file);

    match expr.kind {
        ExprKind::FieldAccess { field, .. } => assert_eq!(field, "map"),
        _ => panic!("Expected FieldAccess"),
    }
}

#[test]
fn test_complex_dot_chaining() {
    let input = r#"
RouteInfo:
    val = x.implementation.(path())
"#;

    let file = parse_file(input).expect("Should parse dot-expression syntax");
    let expr = get_first_field_value(file);
    println!("Parsed: {:?}", expr);
}

#[test]
fn test_binary_equality_chain() {
    let input = r#"
RouteInfo:
    x = a == b == c
"#;
    let result = parse_file(input);
    assert!(
        result.is_ok(),
        "Chained operators shouldn't crash the parser"
    );
}

#[test]
fn test_function_call_parsing() {
    let input = r#"
RouteInfo:
    x = all_paths(arch, 10)
"#;
    let file = parse_file(input).expect("Should parse function call");
    let expr = get_first_field_value(file);

    if let ExprKind::FunctionCall { args, .. } = expr.kind {
        assert_eq!(args.len(), 2);
    } else {
        panic!("Expected FunctionCall");
    }
}

#[test]
fn test_comma_separated_list_no_brackets() {
    let input = r#"
RouteInfo:
    routed_gates = CX, T
"#;
    let file = parse_file(input).expect("Should parse list");
    let expr = get_first_field_value(file);

    if let ExprKind::List(items) = expr.kind {
        assert_eq!(items.len(), 2);
    } else {
        panic!("Phase 2: Expected List, got {:?}", expr.kind);
    }
}

#[test]
fn test_comparison_in_if_condition() {
    let input = r#"RouteInfo:
    realize_gate = if (Gate.gate_type()) == CX then Some(result) else None"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        if let ExprKind::IfThenElse { condition, .. } = &field.value.kind {
            if let ExprKind::BinaryOp { op, left, right } = &condition.kind {
                assert_eq!(*op, BinaryOperator::Eq);
                assert!(matches!(left.kind, ExprKind::FunctionCall { .. }));
                assert!(matches!(right.kind, ExprKind::Identifier(_)));
            } else {
                panic!("Expected BinaryOp in condition, got: {:?}", condition.kind);
            }
        } else {
            panic!("Expected IfThenElse");
        }
    }
}

#[test]
fn test_nested_comparisons() {
    let input = r#"TransitionInfo:
    cost = if x == y && a < b then 1.0 else 0.0"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        if let ExprKind::IfThenElse { condition, .. } = &field.value.kind {
            if let ExprKind::BinaryOp { op, left, right } = &condition.kind {
                assert_eq!(*op, BinaryOperator::And);
                // Left should be x == y
                assert!(matches!(
                    left.kind,
                    ExprKind::BinaryOp {
                        op: BinaryOperator::Eq,
                        ..
                    }
                ));
                // Right should be a < b
                assert!(matches!(
                    right.kind,
                    ExprKind::BinaryOp {
                        op: BinaryOperator::Lt,
                        ..
                    }
                ));
            } else {
                panic!("Expected BinaryOp with And");
            }
        }
    }
}

#[test]
fn test_parenthesized_comparison() {
    let input = r#"RouteInfo:
    value = if (x == y) then true else false"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        if let ExprKind::IfThenElse { condition, .. } = &field.value.kind {
            assert!(matches!(
                condition.kind,
                ExprKind::BinaryOp {
                    op: BinaryOperator::Eq,
                    ..
                }
            ));
        }
    }
}

#[test]
fn test_newline_before_then() {
    let input = r#"RouteInfo:
    realize_gate = if condition
        then result
        else 0"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    assert_eq!(items.len(), 1, "Should parse exactly one field");
    if let Some(BlockItem::Field(field)) = items.first() {
        assert_eq!(field.key, "realize_gate");
        assert!(matches!(field.value.kind, ExprKind::IfThenElse { .. }));
    }
}

#[test]
fn test_newline_before_else() {
    let input = r#"TransitionInfo:
    cost = if x == y then 1.0
        else 0.0"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert!(matches!(field.value.kind, ExprKind::IfThenElse { .. }));
    }
}

#[test]
fn test_newline_before_arrow_in_lambda() {
    let input = r#"RouteInfo:
    realize_gate = map(|x|
        -> result, list)"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        if let ExprKind::FunctionCall { args, .. } = &field.value.kind {
            assert!(matches!(args[0].kind, ExprKind::Lambda { .. }));
        }
    }
}

#[test]
fn test_newline_before_in_keyword() {
    let input = r#"TransitionInfo:
    cost = let x = 1.0
        in x"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert!(matches!(field.value.kind, ExprKind::LetBinding { .. }));
    }
}

#[test]
fn test_generic_type_in_struct_def() {
    let input = r#"RouteInfo:
    GateRealization{path : Vec<Location>}
    realize_gate = Some(value)"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::StructDef(struct_def)) = items.first() {
        assert_eq!(struct_def.name, "GateRealization");
        assert_eq!(struct_def.fields.len(), 1);

        let param = &struct_def.fields[0];
        if let TypeAnnotation::Generic(base, args) = &param.type_annotation {
            assert_eq!(base, "Vec");
            assert_eq!(args.len(), 1);
            assert!(matches!(args[0], TypeAnnotation::Simple(ref s) if s == "Location"));
        } else {
            panic!("Expected Generic type annotation");
        }
    }
}

#[test]
fn test_nested_generics() {
    let input = r#"ArchInfo:
    Arch{rates : Vec<Vec<Float>>}
    get_locations = []"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::StructDef(struct_def)) = items.first() {
        let param = &struct_def.fields[0];
        // Should be Vec<Vec<Float>>
        if let TypeAnnotation::Generic(outer, outer_args) = &param.type_annotation {
            assert_eq!(outer, "Vec");
            if let TypeAnnotation::Generic(inner, inner_args) = &outer_args[0] {
                assert_eq!(inner, "Vec");
                assert!(matches!(inner_args[0], TypeAnnotation::Simple(ref s) if s == "Float"));
            } else {
                panic!("Expected nested generic");
            }
        }
    }
}

#[test]
fn test_method_chain_with_projection() {
    let input = r#"TransitionInfo:
    value = x.implementation.(path())"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert!(matches!(field.value.kind, ExprKind::IndexAccess { .. }));
    }
}

#[test]
fn test_chained_field_and_method_calls() {
    let input = r#"RouteInfo:
    value = obj.field1.method1().field2.method2(arg)"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        assert!(matches!(field.value.kind, ExprKind::FunctionCall { .. }));
    }
}

#[test]
fn test_multiple_projections() {
    let input = r#"TransitionInfo:
    value = tuple.(0).(1)"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        if let ExprKind::Projection { tuple, index } = &field.value.kind {
            assert_eq!(*index, 1);
            assert!(matches!(tuple.kind, ExprKind::Projection { .. }));
        } else {
            panic!("Expected projection");
        }
    }
}

#[test]
fn test_complex_chaining() {
    let input = r#"RouteInfo:
    value = map(|x| -> x.implementation.(path()), gates())"#;

    let file = parse_file(input).unwrap();

    let BlockContent::Fields(items) = &file.blocks[0].content;
    if let Some(BlockItem::Field(field)) = items.first() {
        if let ExprKind::FunctionCall { args, .. } = &field.value.kind {
            if let ExprKind::Lambda { body, .. } = &args[0].kind {
                assert!(matches!(body.kind, ExprKind::IndexAccess { .. }));
            }
        }
    }
}
