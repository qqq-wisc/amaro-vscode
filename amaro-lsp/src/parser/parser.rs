use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while, take_while1},
    character::complete::{char, multispace1, not_line_ending, satisfy, space0},
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

// Value Parsing
fn parse_nested_value(input: &str) -> IResult<&str, &str> {
    recognize(many0(alt((
        parse_rust_embedded,
        delimited(char('['), parse_nested_value, char(']')),
        delimited(char('{'), parse_nested_value, char('}')),
        delimited(char('('), parse_nested_value, char(')')),
        take_while1(|c| c != '[' && c != ']' && c != '{' && c != '}' && c != '(' && c != ')')
    ))))(input)
}

// Field Parsing
pub fn parse_balanced_value(input: &str) -> IResult<&str, &str> {
    recognize(many0(alt((
        parse_rust_embedded,
        delimited(char('['), parse_nested_value, char(']')),
        delimited(char('{'), parse_nested_value, char('}')),
        delimited(char('('), parse_nested_value, char(')')),
        verify(
            take_while1(|c| c != '[' && c != ']' && c != '{' && c != '}' && c != '(' && c != ')' && c != ',' && c != ';'),
            |s: &str| !s.contains('=')
        )
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

    let (input, _) = space0(input)?;

    let (input, _) = match char::<&str, Error<&str>>('=')(input) {
        Ok(res) => res,
        Err(_) => return Ok((input, None)),
    };

    let (input, _) = space0(input)?;

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
            Err(_) => {
                let mut chars = current_input.chars();
                if chars.next().is_some() {
                    current_input = chars.as_str();
                } else { break; }
            }
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

fn is_new_block_start(line: &str) -> bool {
    let trimmed = line.trim_start();

    // if trimmed.contains('=') {
    //     return false;
    // }

    match parse_identifier(trimmed) {
        Ok((rest, _)) => {
            let next_char = rest.trim_start().chars().next();
            // return next_char == Some('[') || next_char == Some(':');
            matches!(next_char, Some('[') | Some(':'))
        },
        Err(_) => false,
    }
}

pub fn consume_remaining_block(input: &str) -> IResult<&str, &str> {
    let mut current = input;

    let mut len = 0;

    loop {
        if current.is_empty() {
            break;
        }
        if is_new_block_start(current) {
            break;
        }

        match not_line_ending::<&str, Error<&str>>(current) {
            Ok((rest, line)) => {
                let line_len = line.len();
                len += line_len;
                
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
    // recognize(many0(verify(
    //     alt((
    //         recognize(pair(not_line_ending, alt((tag("\n"), tag("\r\n"))))),
    //         verify(not_line_ending, |s: &str| !s.is_empty())
    //     )),
    //     |line: &str| {
    //         !is_new_block_start(line)
    //     }
    // )))(input)
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
        let fields = extract_fields(original_input, body_content);

        return Ok((input, Some(Block {
            kind: kind.to_string(),
            range: calc_range(original_input, start_offset, kind.len()),
            fields,
        })));
    }

    if check_bracket.is_ok() {
        let (input, _) = char('[')(input)?;
        let body_start = input.as_ptr() as usize - original_input.as_ptr() as usize;

        let mut depth = 1;
        let mut body_end = body_start;
        let chars: Vec<char> = original_input[body_start..].chars().collect();

        for (i, &c) in chars.iter().enumerate() {
            match c {
                '[' => depth += 1,
                ']' => {
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
        let fields = extract_fields(original_input, inner_body);

        let remaining_input = &original_input[body_end..];
        let (input, _) = char(']')(remaining_input)?;

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
        match whitespace_handler(current_input) {
            Ok((rest, _)) => {
                current_input = rest;
            },
            Err(_) => {  }
        }

        if current_input.is_empty() {
            break;
        }

        match parse_block(input, current_input) {
            Ok((rest, maybe_block)) => {
                if let Some(block) = maybe_block {
                    blocks.push(block);
                    current_input = rest;
                } else {
                    // Unable to parse a block, skip one character to avoid infinite loop
                    let mut chars = current_input.chars();
                    if chars.next().is_some() {
                        current_input = chars.as_str();
                    } else {
                        break;
                    }
                }
            },
            Err(_) => {
                let mut chars = current_input.chars();
                if chars.next().is_some() {
                    current_input = chars.as_str();
                } else {
                    break;
                }
                // if let Some(pos) = current_input.find('\n') {
                //     current_input = &current_input[pos + 1..];
                // } else {
                //     current_input = "";
                // }
            }
        }
    }

    Ok(AmaroFile { blocks })
}
