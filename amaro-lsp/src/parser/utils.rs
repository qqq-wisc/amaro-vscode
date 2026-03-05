use tower_lsp::lsp_types::{Position, Range};

use crate::ast::AmaroFile;
use crate::ast::Expr;
use crate::ast::ExprKind;

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

/// Returns None if that position does not exist in the text.
/// Returns Some if the position exists, with the char
pub fn get_char_at(text: &str, position: Position) -> Option<char> {
    let mut cur_line = 0;
    let mut cur_offset: Option<u32> = Some(0);
    for char in text.chars() {
        if cur_line == position.line
            && cur_offset.is_some()
            && cur_offset.unwrap() == position.character
        {
            return Some(char);
        } else if cur_line > position.line {
            return None;
        } else if char == '\n' {
            cur_line += 1;
            cur_offset = None;
        } else {
            cur_offset = match cur_offset {
                None => Some(0),
                Some(i) => Some(i + 1),
            };
        }
    }
    None
}

/// Given an AmaroFile and a position, extracts the largest expression which
/// contains that position, or errors if none found.
pub fn largest_expr_containing(file: &AmaroFile, position: Position) -> Result<&Expr, String> {
    file.blocks
        .iter()
        .find_map(|block| {
            let crate::ast::BlockContent::Fields(block_items) = &block.content;

            block_items
                .iter()
                .filter_map(|item| match item {
                    crate::ast::BlockItem::Field(field) => Some(field),
                    crate::ast::BlockItem::StructDef(_) => None,
                    crate::ast::BlockItem::ReturnKeyword { .. } => None,
                })
                .find(|field| {
                    field.value_range.start <= position && field.value_range.end > position
                })
        })
        .map_or(
            Err("Could not find an expression containing the given position.".to_string()),
            |field| Ok(&field.value),
        )
}

/// Given an AmaroFile and a position, extracts the smallests expression which
/// contains the position, or errors if none found.
pub fn smallest_expr_containing(file: &AmaroFile, position: Position) -> Result<&Expr, String> {
    // first, get largest expression
    let largest_expr = largest_expr_containing(file, position)?;
    // then, descend until cannot
    match find_smallest_subexpr(largest_expr, position) {
        None => Err("Something went wrong. The position is actually not in any expression. Should fix this case.".to_string()),
        Some(i) => Ok(i)
    }
}

/// Given an AmaroFile and a position, finds a field name at the given position,
/// or None if it doesn't exist.
pub fn field_name_containing(file: &AmaroFile, position: Position) -> Option<(String, Range)> {
    // i know this looks kinda horrible.
    // all this does is find a field (if one exists) which this position overlaps.
    let found_field: Option<&crate::ast::Field> = file.blocks.iter().find_map(|block| {
        match &block.content {
            crate::ast::BlockContent::Fields(block_items) => block_items,
        }
        .iter()
        .filter_map(|elt| match elt {
            crate::ast::BlockItem::Field(f) => Some(f),
            _ => None,
        })
        .find(|field| field.key_range.start <= position && field.key_range.end > position)
    });

    // map to our option
    found_field.map(|field| (field.key.clone(), field.key_range))
}

/// In an expression, finds the smallest subexpression containing the goal position, or None if the goal position isn't even in the original expression.
/// If the goal position is in the original expression, then something will always be returned.
fn find_smallest_subexpr(expr: &Expr, goal_position: Position) -> Option<&Expr> {
    if goal_position >= expr.range.end || goal_position < expr.range.start {
        None
    } else {
        match &expr.kind {
            ExprKind::List(exprs) => exprs
                .iter()
                .find_map(|elt| find_smallest_subexpr(elt, goal_position)),
            ExprKind::Tuple(exprs) => exprs
                .iter()
                .find_map(|elt| find_smallest_subexpr(elt, goal_position)),
            ExprKind::StructLiteral { fields, .. } => fields
                .iter()
                .find_map(|elt| find_smallest_subexpr(&elt.1, goal_position)),
            ExprKind::FunctionCall { function, args } => {
                find_smallest_subexpr(function, goal_position).or(args
                    .iter()
                    .find_map(|elt| find_smallest_subexpr(elt, goal_position)))
            }
            ExprKind::FieldAccess { object, .. } => find_smallest_subexpr(object, goal_position),
            ExprKind::IndexAccess { object, index } => find_smallest_subexpr(object, goal_position)
                .or(find_smallest_subexpr(index, goal_position)),
            ExprKind::Lambda { body, .. } => find_smallest_subexpr(body, goal_position),
            ExprKind::IfThenElse {
                condition,
                then_branch,
                else_branch,
            } => find_smallest_subexpr(condition, goal_position)
                .or(find_smallest_subexpr(then_branch, goal_position))
                .or(find_smallest_subexpr(else_branch, goal_position)),
            ExprKind::LetBinding { value, body, .. } => find_smallest_subexpr(value, goal_position)
                .or(find_smallest_subexpr(body, goal_position)),
            ExprKind::BinaryOp { left, right, .. } => find_smallest_subexpr(left, goal_position)
                .or(find_smallest_subexpr(right, goal_position)),
            ExprKind::UnaryOp { operand, .. } => find_smallest_subexpr(operand, goal_position),
            ExprKind::Some(in_expr) => find_smallest_subexpr(in_expr, goal_position),
            ExprKind::TensorProduct { left, right } => {
                if let Some(v) = find_smallest_subexpr(left, goal_position) {
                    Some(v)
                } else {
                    find_smallest_subexpr(right, goal_position)
                }
            }
            ExprKind::Projection { tuple, .. } => find_smallest_subexpr(tuple, goal_position),
            _ => None,
        }
        .or(Some(expr))
    }
}

/// Given an expression, determines if its range ends at the provided goal
/// position, then recursively explores any child expressions.
/// ## Returns
/// Option containing the subexpression that ends at the goal position.
/// Note that the option might just contain the original expression, if the
/// original expression ends at the given position. A return of None means that
/// no subexpressions ended at the provided position.
pub fn find_finishing_subexpr(expr: &Expr, goal_position: Position) -> Option<&Expr> {
    if expr.range.end == goal_position {
        Some(expr)
    } else if expr.range.start > goal_position || expr.range.end < goal_position {
        None
    } else {
        match &expr.kind {
            ExprKind::List(exprs) => exprs
                .iter()
                .find_map(|elt| find_finishing_subexpr(elt, goal_position)),
            ExprKind::Tuple(exprs) => exprs
                .iter()
                .find_map(|elt| find_finishing_subexpr(elt, goal_position)),
            ExprKind::StructLiteral { fields, .. } => fields
                .iter()
                .find_map(|elt| find_finishing_subexpr(&elt.1, goal_position)),
            ExprKind::FunctionCall { function, args } => {
                find_finishing_subexpr(function, goal_position).or(args
                    .iter()
                    .find_map(|elt| find_finishing_subexpr(elt, goal_position)))
            }
            ExprKind::FieldAccess { object, .. } => find_finishing_subexpr(object, goal_position),
            ExprKind::IndexAccess { object, index } => {
                find_finishing_subexpr(object, goal_position)
                    .or(find_finishing_subexpr(index, goal_position))
            }
            ExprKind::Lambda { body, .. } => find_finishing_subexpr(body, goal_position),
            ExprKind::IfThenElse {
                condition,
                then_branch,
                else_branch,
            } => find_finishing_subexpr(condition, goal_position)
                .or(find_finishing_subexpr(then_branch, goal_position))
                .or(find_finishing_subexpr(else_branch, goal_position)),
            ExprKind::LetBinding { value, body, .. } => {
                find_finishing_subexpr(value, goal_position)
                    .or(find_finishing_subexpr(body, goal_position))
            }
            ExprKind::BinaryOp { left, right, .. } => find_finishing_subexpr(left, goal_position)
                .or(find_finishing_subexpr(right, goal_position)),
            ExprKind::UnaryOp { operand, .. } => find_finishing_subexpr(operand, goal_position),
            ExprKind::Some(in_expr) => find_finishing_subexpr(in_expr, goal_position),
            ExprKind::TensorProduct { left, right } => {
                if let Some(v) = find_finishing_subexpr(left, goal_position) {
                    Some(v)
                } else {
                    find_finishing_subexpr(right, goal_position)
                }
            }
            ExprKind::Projection { tuple, .. } => find_finishing_subexpr(tuple, goal_position),
            _ => None,
        }
    }
}
