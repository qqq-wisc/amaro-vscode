use amaro_lsp::parser::parser::{
    parse_file,
    parse_identifier,
    consume_remaining_block,
    parse_rust_embedded,
};

#[test]
fn test_parse_identifier_valid() {
    assert!(parse_identifier("GateRealization").is_ok());
    assert!(parse_identifier("_internal").is_ok());
    assert!(parse_identifier("RouteInfo").is_ok());
    assert!(parse_identifier("Arch123").is_ok());
}

#[test]
fn test_parse_identifier_invalid() {
    assert!(parse_identifier("123Invalid").is_err());
    assert!(parse_identifier("").is_err());
    assert!(parse_identifier("{{").is_err());
}


#[test]
fn test_parse_rust_embedded_basic() {
    let input = r#"{{
fn test() {
    let x = 5;
}
}}"#;

    let result = parse_rust_embedded(input);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().0, "");
}

#[test]
fn test_parse_rust_embedded_inline() {
    let input = r#"{{ fn foo() { let x = 1; } }}"#;
    assert!(parse_rust_embedded(input).is_ok());
}


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
    assert_eq!(file.blocks[0].kind, "GateRealization");
    assert_eq!(file.blocks[1].kind, "RouteInfo");
    assert_eq!(file.blocks[2].kind, "Transition");
}


#[test]
fn test_file_with_rust_and_let_bindings() {
    let input = r#"{{
fn get_pair_cost(pair: (Location, Location)) -> f64 {
    let cost = 0.0;
    return cost;
}
}}

let end_cost = 80e-6 + 5e-6
let junction_count = abs(col_a - col_b) + 1

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
fn test_whitespace_only() {
    let file = parse_file("   \n\n   ").unwrap();
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
fn test_windows_line_endings() {
    let input = "RouteInfo:\r\n  routed_gates = CX\r\n\r\nTransitionInfo:\r\n  data = test";
    let file = parse_file(input).unwrap();
    assert_eq!(file.blocks.len(), 2);
}

#[test]
fn test_file_no_trailing_newline() {
    let input = r#"GateRealization[
name = 'test'
]"#;  // no newline

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
fn test_consume_stops_at_next_bracket_block() {
    let input = r#"routed_gates = CX
data = test
GateRealization[
name = 'next'
]"#;

    let (rest, _) = consume_remaining_block(input).unwrap();
    assert!(rest.starts_with("GateRealization["));
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
