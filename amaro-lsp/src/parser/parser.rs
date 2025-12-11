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

use crate::ast::{AmaroFile, Block};
use super::utils::calc_range;

// NOM Parsing
pub fn whitespace_handler(input: &str) -> IResult<&str, &str> {
    recognize(many0(alt((
        multispace1,
        recognize(pair(tag("//"), not_line_ending)),
    ))))(input)
}

pub fn parse_rust_embedded(input: &str) -> IResult<&str, &str> {
    // CHANCE OF BREAKING if the embedded rust program is of the form {{ program }}
    recognize(delimited(tag("{{"), take_until("}}"), tag("}}")))(input)
}


pub fn parse_identifier(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        satisfy(|c| c.is_alphabetic() || c == '_'),
        take_while(|c: char| c.is_alphanumeric() || c == '_')
    ))(input)
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
        recognize(pair(not_line_ending, alt((tag("\n"), tag("\r\n"))))),
        |line: &str| {
            let trimmed = line.trim_start();
            let known_blocks = [
                "GateRealization", "Transition", "Architecture", "Arch", "Step",
                "RouteInfo", "TransitionInfo", "ArchInfo", "StateInfo"
            ];
            
            for block in known_blocks {
                if trimmed.starts_with(block) {
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

    let start_offset = input.as_ptr() as usize - original_input.as_ptr() as usize;

    let (input, kind) = parse_identifier(input)?;
    let (input, _) = whitespace_handler(input)?;

    let check_colon: IResult<&str, char, Error<&str>> = peek(char(':'))(input);

    if check_colon.is_ok() {
        let (input, _) = char(':')(input)?;
        let (input, _) = consume_remaining_block(input)?;

        return Ok((input, Some(Block {
            kind: kind.to_string(),
            range: calc_range(original_input, start_offset, kind.len()),
        })));
    }

    let (input, _) = char::<&str, Error<&str>>('[')(input)?;
    
    let (input, _content) = parse_balanced_parenthesis(input)?;
    let (rest, _) = char(']')(input)?;

    Ok((rest, Some(Block {
        kind: kind.to_string(),
        range: calc_range(original_input, start_offset, kind.len()),
    })))
}

pub fn parse_file(input: &str) -> std::result::Result<AmaroFile, ()> {
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
