use nom::{
    IResult,
    branch::alt,
    bytes::complete::{tag, take_while},
    character::complete::{char, multispace1, not_line_ending, satisfy},
    combinator::{map, peek, recognize, verify},
    multi::{many0, separated_list0, separated_list1},
    sequence::{delimited, pair},
};

use nom::error::Error;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use super::expr::parse_expr;
use super::utils::calc_range;
use crate::ast::*;
use crate::info::blocks::BlockName;

// Whitespaces and Comments
pub fn whitespace_handler(input: &str) -> IResult<&str, &str> {
    recognize(many0(alt((
        multispace1,
        recognize(pair(tag("//"), not_line_ending)),
        parse_rust_embedded_robust,
    ))))(input)
}

/// Robust Rust embedded code parser with balanced brace counting
pub fn parse_rust_embedded_robust(input: &str) -> IResult<&str, &str> {
    let (input, _) = tag("{{")(input)?;
    let start = input;

    let mut depth = 1;
    let mut in_string = false;
    let mut in_char = false;
    let mut escape = false;
    let mut bytes_consumed = 0;

    for (i, ch) in input.char_indices() {
        if escape {
            escape = false;
            continue;
        }

        match ch {
            '\\' if in_string || in_char => escape = true,
            '"' if !in_char => in_string = !in_string,
            '\'' if !in_string => in_char = !in_char,
            '{' if !in_string && !in_char => depth += 1,
            '}' if !in_string && !in_char => {
                depth -= 1;
                if depth == 0 {
                    bytes_consumed = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if depth != 0 {
        return Err(nom::Err::Error(Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    }

    let content = &input[..bytes_consumed];
    let (input, _) = tag("}}")(&input[bytes_consumed..])?;

    Ok((input, &start[..content.len()]))
}

pub fn ws<'a, F, O>(f: F) -> impl FnMut(&'a str) -> IResult<&'a str, O>
where
    F: FnMut(&'a str) -> IResult<&'a str, O>,
{
    delimited(whitespace_handler, f, whitespace_handler)
}

// Identifiers and Keywords
pub fn parse_identifier(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        satisfy(|c| c.is_ascii_alphabetic() || c == '_'),
        take_while(|c: char| c.is_ascii_alphanumeric() || c == '_'),
    ))(input)
}

pub fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "if" | "then"
            | "else"
            | "let"
            | "in"
            | "true"
            | "false"
            | "Some"
            | "None"
            | "where"
            | "return"
            | "match"
            | "with"
    )
}

pub fn parse_non_keyword_identifier(input: &str) -> IResult<&str, &str> {
    verify(parse_identifier, |s: &str| !is_keyword(s))(input)
}

// Type Annotations
fn parse_type_annotation(input: &str) -> IResult<&str, TypeAnnotation> {
    alt((parse_function_type, parse_atomic_type))(input)
}

fn parse_simple_type(input: &str) -> IResult<&str, TypeAnnotation> {
    map(parse_identifier, |s: &str| {
        TypeAnnotation::Simple(s.to_string())
    })(input)
}

fn parse_generic_type(input: &str) -> IResult<&str, TypeAnnotation> {
    let (input, name) = parse_identifier(input)?;
    let (input, _) = ws(char('<'))(input)?;
    let (input, type_args) = separated_list1(ws(char(',')), parse_type_annotation)(input)?;
    let (input, _) = ws(char('>'))(input)?;

    Ok((input, TypeAnnotation::Generic(name.to_string(), type_args)))
}

fn parse_tuple_type(input: &str) -> IResult<&str, TypeAnnotation> {
    let (input, _) = char('(')(input)?;
    let (input, types) = separated_list1(ws(char(',')), parse_type_annotation)(input)?;
    let (input, _) = ws(char(')'))(input)?;

    Ok((input, TypeAnnotation::Tuple(types)))
}

fn parse_typed_param<'a>(original_input: &'a str, input: &'a str) -> IResult<&'a str, TypedParam> {
    let start = input.as_ptr() as usize - original_input.as_ptr() as usize;

    let (input, name) = parse_identifier(input)?;
    let (input, _) = ws(char(':'))(input)?;
    let (input, type_ann) = parse_type_annotation(input)?;

    let end = input.as_ptr() as usize - original_input.as_ptr() as usize;

    Ok((
        input,
        TypedParam::new(
            name.to_string(),
            type_ann,
            calc_range(original_input, start, end - start),
        ),
    ))
}

// Struct Definition
fn parse_struct_def<'a>(
    original_input: &'a str,
    input: &'a str,
    _diags: &mut Vec<Diagnostic>,
) -> IResult<&'a str, StructDef> {
    let start = input.as_ptr() as usize - original_input.as_ptr() as usize;

    let (input, name) = parse_identifier(input)?;
    let name_start = start;
    let name_end = input.as_ptr() as usize - original_input.as_ptr() as usize;

    // let (_, _) = peek(ws(char('{')))(input)?;

    let (input, _) = ws(char('{'))(input)?;
    let (input, params) =
        separated_list0(ws(char(',')), |i| parse_typed_param(original_input, i))(input)?;
    let (input, _) = ws(char('}'))(input)?;

    let end = input.as_ptr() as usize - original_input.as_ptr() as usize;

    Ok((
        input,
        StructDef::new(
            name.to_string(),
            calc_range(original_input, name_start, name_end - name_start),
            params,
            calc_range(original_input, start, end - start),
        ),
    ))
}

// Field & Block Parsing
fn parse_field<'a>(
    original_input: &'a str,
    input: &'a str,
    diags: &mut Vec<Diagnostic>,
) -> Result<(&'a str, Field), (&'a str, String)> {
    let input = match whitespace_handler(input) {
        Ok(r) => r.0,
        Err(_) => {
            return Err((
                input,
                "Issue with whitespace handler in field parsing.".to_string(),
            ));
        }
    };

    let key_start = input.as_ptr() as usize - original_input.as_ptr() as usize;
    let (input, key) = match parse_non_keyword_identifier(input) {
        Ok(r) => r,
        Err(_) => {
            return Err((
                input,
                "There was no identifier for the field, or it was a keyword".to_string(),
            ));
        }
    };
    let key_len = key.len();

    let input = match ws(char('='))(input) {
        Ok(r) => r.0,
        Err(_) => return Err((input, "Field had no =".to_string())),
    };

    let val_start = input.as_ptr() as usize - original_input.as_ptr() as usize;
    let (input, first_expr) = match parse_expr(original_input, input, diags) {
        Ok(r) => r,
        Err(_) => {
            return Err((
                input,
                "Could not parse expression in this field".to_string(),
            ));
        }
    };

    // Check for comma-separated list (e.g., routed_gates = CX, T)
    let (input, rest_exprs) = match many0(|i: &'a str| {
        let (i, _) = whitespace_handler(i)?;
        let (i, _) = char(',')(i)?;
        let (i, _) = whitespace_handler(i)?;
        parse_expr(original_input, i, diags)
    })(input)
    {
        Ok(r) => r,
        Err(_) => {
            return Err((
                input,
                "Could not find a comma-separated list in this field.".to_string(),
            ));
        }
    };

    // If there were commas, wrap everything into a List. Else just return the single expression.
    let value_expr = if rest_exprs.is_empty() {
        first_expr
    } else {
        let mut all_exprs = vec![first_expr];
        all_exprs.extend(rest_exprs);
        let val_end = input.as_ptr() as usize - original_input.as_ptr() as usize;
        Expr::new(
            ExprKind::List(all_exprs),
            calc_range(original_input, val_start, val_end - val_start),
        )
    };

    let val_end = input.as_ptr() as usize - original_input.as_ptr() as usize;

    Ok((
        input,
        Field::new(
            key.to_string(),
            calc_range(original_input, key_start, key_len),
            value_expr,
            calc_range(original_input, val_start, val_end - val_start),
        ),
    ))
}

enum BlockItemType {
    StructDef,
    Field,
}

fn determine_block_item_type(input: &str) -> Option<BlockItemType> {
    // let start = input.as_ptr() as usize - original_input.as_ptr() as usize;

    let (input, _name) = match parse_identifier(input) {
        Ok(p) => p,
        Err(_) => return None, // no identifier, not either struct def nor field
    };
    // let name_start = start;
    // let name_end = input.as_ptr() as usize - original_input.as_ptr() as usize;

    if peek(ws(char('{')))(input).is_ok() {
        return Some(BlockItemType::StructDef);
    }

    // if we get here, then we know that it's NOT struct def
    // check if it's trying to be a field
    if peek(ws(char('=')))(input).is_ok() {
        return Some(BlockItemType::Field);
    }

    // neither struct def nor field, but something else. so none
    None
}

/// Parses a block item. A block item is either a field or a struct def.
/// On Ok, gives where the input has advanced to, along with the parsed block item.
/// On Err, gives where the input should be advanced to, along with a String
/// reason for the error.
fn parse_block_item<'a>(
    original_input: &'a str,
    input: &'a str,
    diags: &mut Vec<Diagnostic>,
) -> Result<(&'a str, BlockItem), (&'a str, String)> {
    let input = match whitespace_handler(input) {
        Ok(res) => res.0,
        Err(_) => return Err((input, "Could not resolve whitespace".to_string())),
    };

    // TODO in here, we need to identify whether it should be a struct def or
    // a field, rather than trying both. then, we can report errors from the one
    // that it ought to be.

    match determine_block_item_type(input) {
        Some(BlockItemType::StructDef) => match parse_struct_def(original_input, input, diags) {
            Ok((input, struct_def)) => Ok((input, BlockItem::StructDef(struct_def))),
            Err(_) => Err((
                input,
                "Could not finish parsing this struct def".to_string(),
            )),
        },
        Some(BlockItemType::Field) => match parse_field(original_input, input, diags) {
            Ok((input, field)) => Ok((input, BlockItem::Field(field))),
            Err((rest, reason)) => {
                Err((rest, format!("Could not parse field. Reason: {}", reason)))
            }
        },
        None => Err((
            input,
            "This is neither a struct definition nor a field.".to_string(),
        )),
    }

    // if let Ok((input, struct_def)) = parse_struct_def(original_input, input) {
    //     eprintln!("Block item parsed as struct def");
    //     return Ok((input, Some(BlockItem::StructDef(struct_def))));
    // }

    // if let Ok((input, field)) = parse_field(original_input, input) {
    //     return Ok((input, Some(BlockItem::Field(field))))
    // }

    // Ok((input, None))
}

fn extract_block_items(
    original_input: &str,
    body_text: &str,
    diags: &mut Vec<Diagnostic>,
) -> Vec<BlockItem> {
    let mut items = Vec::new();
    let mut current_input = body_text;

    while !current_input.trim().is_empty() {
        // Pre-check: detect `key = return ...` before the parse attempt.
        // `parse_field` would silently fail on `return` (it's a keyword), causing
        // a spurious "missing required field" error downstream.
        // Instead, capture it as a ReturnKeyword item so semantics can warn clearly.
        let trimmed = current_input.trim_start();
        if let Some((key_part, after_eq)) = trimmed.split_once('=') {
            let key = key_part.trim();
            let val = after_eq.trim_start();
            let is_simple_identifier = !key.is_empty()
                && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !key.contains(' ');
            let starts_with_return = val == "return"
                || val.starts_with("return ")
                || val.starts_with("return\n")
                || val.starts_with("return\r");
            if is_simple_identifier && starts_with_return {
                let return_offset = val.as_ptr() as usize - original_input.as_ptr() as usize;
                let return_range = calc_range(original_input, return_offset, "return".len());
                items.push(BlockItem::ReturnKeyword {
                    range: return_range,
                    key: key.to_string(),
                });
                if let Some(pos) = current_input.find('\n') {
                    current_input = &current_input[pos + 1..];
                } else {
                    break;
                }
                continue;
            }
        }

        match parse_block_item(original_input, current_input, diags) {
            Ok((rest, item)) => {
                items.push(item);
                current_input = rest;
            }
            Err((rest, reason)) => {
                if let Some(pos) = rest.find('\n') {
                    current_input = &rest[pos + 1..];
                    diags.push(Diagnostic {
                        range: calc_range(
                            original_input,
                            rest.as_ptr() as usize - original_input.as_ptr() as usize,
                            pos,
                        ),
                        severity: Some(DiagnosticSeverity::ERROR),
                        source: Some("Parser".to_string()),
                        message: reason,
                        ..Default::default()
                    });
                } else {
                    break;
                }
            }
        }
    }

    items
}

fn is_new_block_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    match parse_identifier(trimmed) {
        Ok((rest, name)) => {
            let next_char = rest.trim_start().chars().next();
            matches!(next_char, Some('['))
                || (matches!(next_char, Some(':')) && BlockName::from_string(name).is_some())
        }
        Err(_) => false,
    }
}

pub fn consume_remaining_block(input: &str) -> IResult<&str, &str> {
    let mut current = input;
    let mut len = 0;

    loop {
        if current.is_empty() || is_new_block_start(current) {
            break;
        }
        match not_line_ending::<&str, Error<&str>>(current) {
            Ok((rest, line)) => {
                len += line.len();
                current = rest;

                if let Ok((rest_nl, nl)) =
                    alt::<_, _, Error<&str>, _>((tag("\n"), tag("\r\n")))(current)
                {
                    len += nl.len();
                    current = rest_nl;
                } else {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    Ok((current, &input[..len]))
}

pub fn parse_block<'a>(
    original_input: &'a str,
    input: &'a str,
    diags: &mut Vec<Diagnostic>,
) -> IResult<&'a str, Option<Block>> {
    let (input, _) = whitespace_handler(input)?;
    if input.is_empty() {
        return Ok((input, None));
    }

    let start_offset = input.as_ptr() as usize - original_input.as_ptr() as usize;

    let (input, kind) = parse_identifier(input)?;
    let (input, _) = whitespace_handler(input)?;

    let check_colon: IResult<&str, char, Error<&str>> = peek(char(':'))(input);
    let check_bracket: IResult<&str, char, Error<&str>> = peek(char('['))(input);

    if check_colon.is_ok() {
        let (input, _) = char(':')(input)?;
        let (input, body_content) = consume_remaining_block(input)?;
        let items = extract_block_items(original_input, body_content, diags);

        return Ok((
            input,
            Some(Block::new(
                kind.to_string(),
                calc_range(original_input, start_offset, kind.len()),
                BlockContent::Fields(items),
            )),
        ));
    }

    if check_bracket.is_ok() {
        let (input, _) = char('[')(input)?;
        let body_start = input.as_ptr() as usize - original_input.as_ptr() as usize;

        let bytes = &original_input.as_bytes()[body_start..];
        let mut depth = 1;
        let mut body_end = body_start;

        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        body_end = body_start + i;
                        break;
                    }
                }
                _ => {}
            }
        }

        let inner_body = &original_input[body_start..body_end];
        let items = extract_block_items(original_input, inner_body, diags);

        let remaining_input = &original_input[body_end..];
        let (input, _) = char(']')(remaining_input)?;

        return Ok((
            input,
            Some(Block::new(
                kind.to_string(),
                calc_range(original_input, start_offset, kind.len()),
                BlockContent::Fields(items),
            )),
        ));
    }

    Ok((input, None))
}

fn parse_atomic_type(input: &str) -> IResult<&str, TypeAnnotation> {
    alt((parse_generic_type, parse_tuple_type, parse_simple_type))(input)
}

// Parse function
fn parse_function_type(input: &str) -> IResult<&str, TypeAnnotation> {
    let (input, params) = separated_list1(ws(char(',')), parse_atomic_type)(input)?;
    let (input, _) = ws(alt((tag("->"), tag("→"))))(input)?;

    let (input, return_type) = parse_type_annotation(input)?;
    Ok((
        input,
        TypeAnnotation::Function {
            params,
            return_type: Box::new(return_type),
        },
    ))
}

pub struct ParseOutput {
    pub file: AmaroFile,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse_file(input: &str) -> std::result::Result<ParseOutput, String> {
    // Commented since this was causing race condition in tests
    // reset_node_ids();

    let mut blocks = Vec::new();
    let mut current_input = input;
    let mut parse_diagnostics = Vec::new();

    while !current_input.is_empty() {
        // Skip whitespace
        if let Ok((rest, _)) = whitespace_handler(current_input) {
            current_input = rest;
        }

        if current_input.is_empty() {
            break;
        }

        match parse_block(input, current_input, &mut parse_diagnostics) {
            Ok((rest, Some(block))) => {
                blocks.push(block);
                current_input = rest;
            }
            Ok((rest, None)) => {
                // Parsed successfully but got nothing. Advance
                current_input = rest;
            }
            Err(e) => {
                let new_input = match e {
                    nom::Err::Incomplete(_) => current_input,
                    nom::Err::Error(next) => next.input,
                    nom::Err::Failure(next) => next.input,
                };
                // Error recovery

                // the error recovery method here is to silently move to next line
                if let Some(pos) = new_input.find('\n') {
                    current_input = &new_input[pos + 1..];
                } else {
                    // Skip one character
                    let mut chars = current_input.chars();
                    if chars.next().is_some() {
                        current_input = chars.as_str();
                    } else {
                        break;
                    }
                }
            }
        }
    }

    Ok(ParseOutput {
        file: AmaroFile::new(blocks),
        diagnostics: parse_diagnostics,
    })
}
