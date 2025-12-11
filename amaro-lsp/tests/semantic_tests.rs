use amaro_lsp::parser::{parse_file, check_semantics};

#[test]
fn capitalization_warning() {
    let input = "architecture[1]";
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    assert_eq!(diags.len(), 1);
}

#[test]
fn no_warning_for_correct_capitalization() {
    let input = "Architecture[1]";
    let file = parse_file(input).unwrap();
    let diags = check_semantics(&file);
    assert!(diags.is_empty());
}
