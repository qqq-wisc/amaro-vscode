use amaro_lsp::parser::utils::{byte_to_position, calc_range};

#[test]
fn test_byte_to_position_single_line() {
    let text = "GateRealization[name='test']";
    let (line, col) = byte_to_position(text, 0);
    assert_eq!(line, 0);
    assert_eq!(col, 0);

    let (line, col) = byte_to_position(text, 15);
    assert_eq!(line, 0);
    assert_eq!(col, 15);
}

#[test]
fn test_byte_to_position_multiline() {
    let text = "Line1\nLine2\nLine3";
    let (line, col) = byte_to_position(text, 0);
    assert_eq!(line, 0);
    assert_eq!(col, 0);

    let (line, col) = byte_to_position(text, 6); // Start of Line2
    assert_eq!(line, 1);
    assert_eq!(col, 0);

    let (line, col) = byte_to_position(text, 12); // Start of Line3
    assert_eq!(line, 2);
    assert_eq!(col, 0);
}

#[test]
fn test_calc_range() {
    let text = "GateRealization[name='test']";
    let range = calc_range(text, 0, 15); // "GateRealization"
    assert_eq!(range.start.line, 0);
    assert_eq!(range.start.character, 0);
    assert_eq!(range.end.line, 0);
    assert_eq!(range.end.character, 15);
}
