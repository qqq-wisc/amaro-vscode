use amaro_lsp::parser::parser::{
    parse_file
};

const FILE_1: &str = include_str!("../test_files/file1.qmrl");
const FILE_3: &str = include_str!("../test_files/file3.qmrl");
const FILE_5: &str = include_str!("../test_files/file5.qmrl");

#[test]
fn test_file_1_parsing() {
    let result = parse_file(FILE_1);
    assert!(result.is_ok());
    let file = result.unwrap();
    assert!(file.blocks.len() >= 3); // At least GateRealization, Transition, Architecture
}

#[test]
fn test_file_3_parsing() {
    let result = parse_file(FILE_3);
    assert!(result.is_ok());
    let file = result.unwrap();
    assert_eq!(file.blocks.len(), 3); // GateRealization, Transition, Architecture
}

#[test]
fn test_file_5_parsing() {
    let result = parse_file(FILE_5);
    assert!(result.is_ok());
}

