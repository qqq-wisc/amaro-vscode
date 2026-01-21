use nom::{
    branch::alt,
    bytes::complete::{tag, take_while},
    character::complete::{char, multispace1, not_line_ending, satisfy},
    combinator::{map, peek, recognize, verify},
    multi::{many0, separated_list0, separated_list1},
    sequence::{delimited, pair},
    IResult,
};

use nom::error::Error;

use crate::ast::*;
use super::utils::calc_range;
use super::expr::parse_expr;

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
        return Err(nom::Err::Error(Error::new(input, nom::error::ErrorKind::Tag)));
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
        take_while(|c: char| c.is_ascii_alphanumeric() || c == '_')
    ))(input)
}

pub fn is_keyword(s: &str) -> bool {
    matches!(s, "if" | "then" | "else" | "let" | "in" | "true" | "false" 
        | "Some" | "None" | "where" | "return")
}

pub fn parse_non_keyword_identifier(input: &str) -> IResult<&str, &str> {
    verify(parse_identifier, |s: &str| !is_keyword(s))(input)
}

// Type Annotations
fn parse_type_annotation(input: &str) -> IResult<&str, TypeAnnotation> {
    alt((
        parse_generic_type,
        parse_tuple_type,
        parse_simple_type,
    ))(input)
}

fn parse_simple_type(input: &str) -> IResult<&str, TypeAnnotation> {
    map(parse_identifier, |s: &str| TypeAnnotation::Simple(s.to_string()))(input)
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
    
    Ok((input, TypedParam::new(
        name.to_string(),
        type_ann,
        calc_range(original_input, start, end - start),
    )))
}

// Struct Definition
fn parse_struct_def<'a>(original_input: &'a str, input: &'a str) -> IResult<&'a str, StructDef> {
    let start = input.as_ptr() as usize - original_input.as_ptr() as usize;
    
    let (input, name) = parse_identifier(input)?;
    let name_start = start;
    let name_end = input.as_ptr() as usize - original_input.as_ptr() as usize;
    
    let (_, _) = peek(ws(char('{')))(input)?;
    
    let (input, _) = ws(char('{'))(input)?;
    let (input, params) = separated_list0(
        ws(char(',')),
        |i| parse_typed_param(original_input, i)
    )(input)?;
    let (input, _) = ws(char('}'))(input)?;
    
    let end = input.as_ptr() as usize - original_input.as_ptr() as usize;
    
    Ok((input, StructDef::new(
        name.to_string(),
        calc_range(original_input, name_start, name_end - name_start),
        params,
        calc_range(original_input, start, end - start),
    )))
}

// Field & Block Parsing
fn parse_field<'a>(original_input: &'a str, input: &'a str) -> IResult<&'a str, Field> {
    let (input, _) = whitespace_handler(input)?;
    
    let key_start = input.as_ptr() as usize - original_input.as_ptr() as usize;
    let (input, key) = parse_non_keyword_identifier(input)?;
    let key_len = key.len();
    
    let (input, _) = ws(char('='))(input)?;
    
    let val_start = input.as_ptr() as usize - original_input.as_ptr() as usize;
    let (input, value_expr) = parse_expr(original_input, input)?;
    let val_end = input.as_ptr() as usize - original_input.as_ptr() as usize;
    
    Ok((input, Field::new(
        key.to_string(),
        calc_range(original_input, key_start, key_len),
        value_expr,
        calc_range(original_input, val_start, val_end - val_start),
    )))
}

fn parse_block_item<'a>(original_input: &'a str, input: &'a str) -> IResult<&'a str, Option<BlockItem>> {
    let (input, _) = whitespace_handler(input)?;

    if let Ok((input, struct_def)) = parse_struct_def(original_input, input) {
        return Ok((input, Some(BlockItem::StructDef(struct_def))));
    }

    if let Ok((input, field)) = parse_field(original_input, input) {
        return Ok((input, Some(BlockItem::Field(field))));
    }

    Ok((input, None))
}

fn extract_block_items(original_input: &str, body_text: &str) -> Vec<BlockItem> {
    let mut items = Vec::new();
    let mut current_input = body_text;

    while !current_input.trim().is_empty() {
        match parse_block_item(original_input, current_input) {
            Ok((rest, Some(item))) => {
                items.push(item);
                current_input = rest;
            },
            Ok((_rest, None)) => {
                if let Some(pos) = current_input.find('\n') {
                    current_input = &current_input[pos + 1..];
                } else {
                    break;
                }
            },
            Err(_) => {
                if let Some(pos) = current_input.find('\n') {
                    current_input = &current_input[pos + 1..];
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
        Ok((rest, _)) => {
            let next_char = rest.trim_start().chars().next();
            matches!(next_char, Some('[') | Some(':'))
        },
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
                
                if let Ok((rest_nl, nl)) = alt::<_, _, Error<&str>, _>((tag("\n"), tag("\r\n")))(current) {
                    len += nl.len();
                    current = rest_nl;
                } else {
                    break;
                }
            },
            Err(_) => break,
        }
    }

    Ok((current, &input[..len]))
}

pub fn parse_block<'a>(original_input: &'a str, input: &'a str) -> IResult<&'a str, Option<Block>> {
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
        let items = extract_block_items(original_input, body_content);
        
        return Ok((input, Some(Block::new(
            kind.to_string(),
            calc_range(original_input, start_offset, kind.len()),
            BlockContent::Fields(items),
        ))));
    }

    if check_bracket.is_ok() {
        let (input, _) = char('[')(input)?;
        let body_start = input.as_ptr() as usize - original_input.as_ptr() as usize;
        
        let bytes = original_input[body_start..].as_bytes();
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
        let items = extract_block_items(original_input, inner_body);
        
        let remaining_input = &original_input[body_end..];
        let (input, _) = char(']')(remaining_input)?;
        
        return Ok((input, Some(Block::new(
            kind.to_string(),
            calc_range(original_input, start_offset, kind.len()),
            BlockContent::Fields(items),
        ))));
    }

    Ok((input, None))
}

pub fn parse_file(input: &str) -> std::result::Result<AmaroFile, String> {
    // Commented since this was causing race condition in tests
    // reset_node_ids();

    let mut blocks = Vec::new();
    let mut current_input = input;

    while !current_input.is_empty() {
        // Skip whitespace
        match whitespace_handler(current_input) {
            Ok((rest, _)) => current_input = rest,
            Err(_) => {}
        }
        
        if current_input.is_empty() {
            break;
        }

        match parse_block(input, current_input) {
            Ok((rest, Some(block))) => {
                blocks.push(block);
                current_input = rest;
            },
            Ok((rest, None)) => {
                // Parsed successfully but got nothing. Advance
                current_input = rest;
            },
            Err(_) => {
                // Error recovery
                if let Some(pos) = current_input.find('\n') {
                    current_input = &current_input[pos + 1..];
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

    Ok(AmaroFile::new(blocks))
}
