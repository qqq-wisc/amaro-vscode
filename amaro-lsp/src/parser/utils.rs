use tower_lsp::lsp_types::{Position, Range};

pub fn calc_range(full_text: &str, start_offset: usize, length: usize) -> Range {
    let abs_start = start_offset;
    let abs_end = start_offset + length;

    let (start_line, start_col) = byte_to_position(full_text, abs_start);
    let (end_line, end_col) = byte_to_position(full_text, abs_end);

    Range {
        start: Position {
            line: start_line,
            character: start_col,
        },
        end: Position {
            line: end_line,
            character: end_col,
        },
    }
}

pub fn byte_to_position(text: &str, byte_idx: usize) -> (u32, u32) {
    let safe_idx = std::cmp::min(byte_idx, text.len());
    let slice = &text[..safe_idx];

    let line = slice.matches('\n').count() as u32;
    let last_line_start = slice.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = (safe_idx - last_line_start) as u32;
    (line, col)
}
