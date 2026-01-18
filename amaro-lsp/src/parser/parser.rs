use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while, take_while1},
    character::complete::{char, multispace1, not_line_ending, satisfy},
    combinator::{peek, recognize, verify},
    multi::many0,
    sequence::{delimited, pair},
    IResult,
};

use nom::error::Error;

use crate::ast::*;
use super::utils::calc_range;

// NOM Parsing
pub fn whitespace_handler(input: &str) -> IResult<&str, &str> {
    recognize(many0(alt((
        multispace1,
        recognize(pair(tag("//"), not_line_ending)),
        parse_rust_embedded,
    ))))(input)
}

pub fn parse_rust_embedded(input: &str) -> IResult<&str, &str> {
    // CHANCE OF BREAKING if the embedded rust program is of the form {{ program }}
    recognize(delimited(tag("{{"), take_until("}}"), tag("}}")))(input)
}


pub fn parse_identifier(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        satisfy(|c| c.is_ascii_alphabetic() || c == '_'),
        take_while(|c: char| c.is_ascii_alphanumeric() || c == '_')
    ))(input)
}

// Field Parsing
pub fn parse_balanced_value(input: &str) -> IResult<&str, &str> {
    recognize(many0(alt((
        parse_rust_embedded,
        delimited(char('['), parse_balanced_value, char(']')),
        delimited(char('{'), parse_balanced_value, char('}')),
        delimited(char('('), parse_balanced_value, char(')')),
        take_while1(|c| c != '[' && c != ']' && c != '{' && c != '}' && c != '(' && c != ')' && c != '\n' && c != ',' && c != '=' && c != ';')
    ))))(input)

}

fn parse_field<'a>(original_full_text: &'a str, input: &'a str) -> IResult<&'a str, Option<Field>> {
    let (input, _) = whitespace_handler(input)?;
    
    let key_start = input.as_ptr() as usize - original_full_text.as_ptr() as usize;
    let (input, key) = match parse_identifier(input) {
        Ok(res) => res,
        Err(_) => return Ok((input, None)), 
    };
    let key_len = key.len();

    let (input, _) = whitespace_handler(input)?;

    let (input, _) = match char::<&str, Error<&str>>('=')(input) {
        Ok(res) => res,
        Err(_) => return Ok((input, None)),
    };

    let (input, _) = whitespace_handler(input)?;

    let val_start = input.as_ptr() as usize - original_full_text.as_ptr() as usize;
    let (input, val_raw) = parse_balanced_value(input)?;
    let val_len = val_raw.len();

    let (input, _) = many0(alt((char(','), char(';'))))(input)?;

    Ok((input, Some(Field {
        key: key.to_string(),
        key_range: calc_range(original_full_text, key_start, key_len),
        value: val_raw.trim().to_string(),
        value_range: calc_range(original_full_text, val_start, val_len),
    })))
}

fn extract_fields(original_full_text: &str, body_text: &str) -> Vec<Field> {
    let mut fields = Vec::new();
    let mut current_input = body_text;

    while !current_input.trim().is_empty() {
        match parse_field(original_full_text, current_input) {
            Ok((rest, maybe_field)) => {
                if let Some(field) = maybe_field {
                    fields.push(field);
                    current_input = rest;
                } else {
                    let mut chars = current_input.chars();
                    if chars.next().is_some() {
                        current_input = chars.as_str();
                    } else { break; }
                }
            },
            Err(_) => break,
        }
    }
    fields
}

pub fn parse_balanced_parenthesis(input: &str) -> IResult<&str, &str> {
    recognize(many0(alt((
        parse_rust_embedded,
        delimited(char('['), parse_balanced_parenthesis, char(']')),
        delimited(char('{'), parse_balanced_parenthesis, char('}')),
        delimited(char('('), parse_balanced_parenthesis, char(')')),

        take_while1(|c| c != '[' && c != ']' && c != '{' && c != '}' && c != '(' && c != ')')
    ))))(input)
}

pub fn consume_remaining_block(input: &str) -> IResult<&str, &str> {
    recognize(many0(verify(
        alt((
            recognize(pair(not_line_ending, alt((tag("\n"), tag("\r\n"))))),
            verify(not_line_ending, |s: &str| !s.is_empty()),
        )),
        |line: &str| {
            let trimmed = line.trim_start();
            let known_blocks = [
                "GateRealization", "Transition", "Architecture", "Arch", "Step",
                "RouteInfo", "TransitionInfo", "ArchInfo", "StateInfo"
            ];
            
            for block in known_blocks {
                if trimmed.len() >= block.len() && trimmed[..block.len()].eq_ignore_ascii_case(block) {
                    let after_block = &trimmed[block.len()..];
                    let next_char = after_block.trim_start().chars().next();

                    if next_char == Some('[') || next_char == Some(':') {
                        return false;
                    }
                }
            }
            true
        }
    )))(input)
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

    if check_colon.is_ok() {
        let (input, _) = char(':')(input)?;
        let (input, body_content) = consume_remaining_block(input)?;
        let fields = extract_fields(original_input, body_content);

        return Ok((input, Some(Block {
            kind: kind.to_string(),
            range: calc_range(original_input, start_offset, kind.len()),
            fields,
        })));
    }

    if let Ok((input_after_bracket, _)) = char::<&str, Error<&str>>('[')(input) {
         let body_start = input_after_bracket.as_ptr() as usize - original_input.as_ptr() as usize;
         let (input, _) = parse_balanced_parenthesis(input_after_bracket)?;
         let body_end = input.as_ptr() as usize - original_input.as_ptr() as usize;
         
         let inner_body = &original_input[body_start..body_end];
         let fields = extract_fields(original_input, inner_body);

         let (input, _) = char(']')(input)?;

         return Ok((input, Some(Block {
            kind: kind.to_string(),
            range: calc_range(original_input, start_offset, kind.len()),
            fields,
        })));
    }

    Ok((input, None))
}

pub fn parse_file(input: &str) -> std::result::Result<AmaroFile, String> {
    let mut blocks = Vec::new();
    let mut current_input = input;

    while !current_input.is_empty() {
        match parse_block(input, current_input) {
            Ok((rest, maybe_block)) => {
                if let Some(block) = maybe_block {
                    blocks.push(block);
                }
                current_input = rest;
            },
            Err(_) => {
                if let Some(pos) = current_input.find('\n') {
                    current_input = &current_input[pos + 1..];
                } else {
                    current_input = "";
                }
            }
        }
    }

    Ok(AmaroFile { blocks })
}
