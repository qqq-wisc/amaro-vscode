use amaro_lsp::parser::{
    parse_file,
    parse_rust_embedded_robust,
    parse_identifier,
    consume_remaining_block
};
use amaro_lsp::ast::*;

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
        if let ExprKind::IfThenElse { condition, then_branch, else_branch } = &field.value.kind {
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

    let BlockItem::Field(f1) = field1 else { panic!("Expected field") };
    let BlockItem::Field(f2) = field2 else { panic!("Expected field") };

    let ExprKind::List(gates1) = &f1.value.kind else { 
        panic!("No brackets: Expected List, got: {:?}", f1.value.kind) 
    };
    let ExprKind::List(gates2) = &f2.value.kind else { 
        panic!("With brackets: Expected List, got: {:?}", f2.value.kind) 
    };

    assert_eq!(gates1.len(), 3);
    assert_eq!(gates2.len(), 3);

    for (g1, g2) in gates1.iter().zip(gates2.iter()) {
        let ExprKind::Identifier(name1) = &g1.kind else { panic!("Expected identifier") };
        let ExprKind::Identifier(name2) = &g2.kind else { panic!("Expected identifier") };
        assert_eq!(name1, name2);
    }
}
