use std::collections::VecDeque;

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
#[cfg(test)]
pub fn get_char_at(text: &str, position: Position) -> Option<char> {
    let mut cur_line = 0;
    let mut cur_offset = 0;
    for char in text.chars() {
        if cur_line == position.line && cur_offset == position.character {
            return Some(char);
        } else if cur_line > position.line {
            return None;
        } else if char == '\n' {
            cur_line += 1;
            cur_offset = 0
        } else {
            cur_offset += 1;
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

/// Given an AmaroFile and a position, finds a field name at the given position
/// and the block name, or None if it doesn't exist.
/// First elt: Block name
/// Second elt: Field name
/// Third elt: Range of field
pub fn field_name_containing(
    file: &AmaroFile,
    position: Position,
) -> Option<(String, String, Range)> {
    // i know this looks kinda horrible.
    // all this does is find a field (if one exists) which this position overlaps.
    let found_tuple: Option<(&crate::ast::Block, &crate::ast::Field)> =
        file.blocks.iter().find_map(|block| {
            match &block.content {
                crate::ast::BlockContent::Fields(block_items) => block_items,
            }
            .iter()
            .filter_map(|elt| match elt {
                crate::ast::BlockItem::Field(f) => Some((block, f)),
                _ => None,
            })
            .find(|(_, field)| field.key_range.start <= position && field.key_range.end > position)
        });

    // map to our option
    found_tuple.map(|(block, field)| (block.kind.clone(), field.key.clone(), field.key_range))
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

/// Given an expression, finds the smallest expression or subexpression that
/// ends at the provided goal_position, with a liiiiittle leniency
/// ## Returns
/// Option containing the subexpression that ends at the goal position.
/// Note that the option might just contain the original expression, if the
/// original expression ends at the given position. A return of None means that
/// no subexpressions ended at the provided position.
///
///
pub fn find_finishing_subexpr(expr: &Expr, goal_position: Position) -> Option<&Expr> {
    let mut exprs_to_check: VecDeque<&Expr> = VecDeque::new();
    let mut best_expr: Option<&Expr> = None;

    // give 1 character of leniency...
    // ideally, remove this and just use the exact end position. however, some ranges
    // are very slightly off, so this makes things easier than picking through all
    // the weeds. i dont think this will ever be problematic because expressions have a min size of 1 anyway.
    let pos_before_goal = Position::new(
        goal_position.line,
        goal_position.character.saturating_sub(1),
    );

    exprs_to_check.push_back(expr);

    while let Some(current) = exprs_to_check.pop_front() {
        if current.range.end == goal_position || current.range.end == pos_before_goal {
            best_expr = Some(current);

            // we just take the most recent expression as the best one.
            // this is because our system here automatically evaluates from
            // largest to smallest, so we always know the most recent expression
            // we find will be the best.
        }
        if current.range.start <= goal_position && current.range.end >= goal_position {
            match &current.kind {
                ExprKind::List(exprs) => exprs_to_check.extend(exprs),
                ExprKind::Tuple(exprs) => exprs_to_check.extend(exprs),
                ExprKind::StructLiteral { fields, .. } => {
                    exprs_to_check.extend(fields.iter().map(|elt| &elt.1))
                }
                ExprKind::FunctionCall { function, args } => {
                    exprs_to_check.push_back(function);
                    exprs_to_check.extend(args.iter());
                }
                ExprKind::FieldAccess { object, .. } => exprs_to_check.push_back(object),
                ExprKind::IndexAccess { object, index } => {
                    exprs_to_check.push_back(object);
                    exprs_to_check.push_back(index);
                }
                ExprKind::Lambda { body, .. } => exprs_to_check.push_back(body),
                ExprKind::IfThenElse {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    exprs_to_check.push_back(condition);
                    exprs_to_check.push_back(then_branch);
                    exprs_to_check.push_back(else_branch);
                }
                ExprKind::LetBinding { value, body, .. } => {
                    exprs_to_check.push_back(value);
                    exprs_to_check.push_back(body);
                }
                ExprKind::BinaryOp { left, right, .. } => {
                    exprs_to_check.push_back(left);
                    exprs_to_check.push_back(right);
                }
                ExprKind::UnaryOp { operand, .. } => exprs_to_check.push_back(operand),
                ExprKind::Some(in_expr) => exprs_to_check.push_back(in_expr),
                ExprKind::TensorProduct { left, right } => {
                    exprs_to_check.push_back(left);
                    exprs_to_check.push_back(right);
                }
                ExprKind::Projection { tuple, .. } => exprs_to_check.push_back(tuple),
                ExprKind::Match { scrutinee, arms } => {
                    exprs_to_check.push_back(scrutinee);
                    exprs_to_check.extend(arms.iter().map(|elt| &elt.body));
                }
                ExprKind::Identifier(_)
                | ExprKind::IntLiteral(_)
                | ExprKind::FloatLiteral(_)
                | ExprKind::StringLiteral(_)
                | ExprKind::BoolLiteral(_)
                | ExprKind::None => { /* cant descend further */ }
            }
        } else {
            // reject

            // eprintln!(
            //     "\t\tREJECT: Start {:?} end {:?} goal {:?}",
            //     current.range.start, current.range.end, goal_position
            // )
        }
    }

    best_expr
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_get_char_at() {
        let text = "abc\ndef";

        assert_eq!(Some('a'), get_char_at(text, Position::new(0, 0)));
        assert_eq!(Some('b'), get_char_at(text, Position::new(0, 1)));
        assert_eq!(Some('d'), get_char_at(text, Position::new(1, 0)));
    }
}
