use nom::{
    branch::alt,
    bytes::complete::{tag, take_while},
    character::complete::{char, digit1},
    combinator::{map, opt, peek, recognize, value},
    multi::{many0, separated_list0},
    sequence::{pair, terminated, tuple},
    IResult,
};
use nom::error::Error;

use crate::ast::*;
use super::utils::calc_range;

use super::parser::{
    ws, 
    parse_identifier, 
    parse_non_keyword_identifier, 
    is_keyword,
    whitespace_handler
};

const MAX_RECURSION_DEPTH: usize = 100;

// Expression Parsing
struct ParseContext {
    depth: usize,
}

impl ParseContext {
    fn new() -> Self {
        ParseContext { depth: 0 }
    }
    
    fn check_depth(&self) -> Result<(), nom::Err<Error<&'static str>>> {
        if self.depth >= MAX_RECURSION_DEPTH {
            Err(nom::Err::Error(Error::new("", nom::error::ErrorKind::TooLarge)))
        } else {
            Ok(())
        }
    }
    
    fn enter(&mut self) -> Result<(), nom::Err<Error<&'static str>>> {
        self.check_depth()?;
        self.depth += 1;
        Ok(())
    }
    
    fn exit(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

pub fn parse_expr<'a>(original_input: &'a str, input: &'a str) -> IResult<&'a str, Expr> {
    let (input, _) = whitespace_handler(input)?;

    let mut ctx = ParseContext::new();
    parse_expr_with_context(original_input, input, &mut ctx)
}

fn parse_expr_with_context<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    ctx.enter().map_err(|_| nom::Err::Error(Error::new(input, nom::error::ErrorKind::TooLarge)))?;
    let result = parse_let_expr(original_input, input, ctx);
    ctx.exit();
    result
}

fn parse_let_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    let start = input.as_ptr() as usize - original_input.as_ptr() as usize;
    
    // 1. Consume whitespace before 'let'
    let (input, _) = whitespace_handler(input)?;
    let (input, is_let) = opt(tag("let"))(input)?;
    
    if is_let.is_some() {
        // 2. Whitespace after 'let'
        let (input, _) = whitespace_handler(input)?;
        let (input, name) = parse_non_keyword_identifier(input)?;
        
        // 3. Handle '=' with whitespace around it
        let (input, _) = whitespace_handler(input)?;
        let (input, _) = char('=')(input)?;
        let (input, _) = whitespace_handler(input)?;
        
        let (input, value) = parse_if_expr(original_input, input, ctx)?;
        
        // 4. Handle 'in' with whitespace around it
        let (input, _) = whitespace_handler(input)?;
        let (input, _) = tag("in")(input)?;
        let (input, _) = whitespace_handler(input)?;

        let (input, body) = parse_expr_with_context(original_input, input, ctx)?;
        
        let end = input.as_ptr() as usize - original_input.as_ptr() as usize;
        
        Ok((input, Expr::new(
            ExprKind::LetBinding {
                name: name.to_string(),
                value: Box::new(value),
                body: Box::new(body),
            },
            calc_range(original_input, start, end - start)
        )))
    } else {
        parse_if_expr(original_input, input, ctx)
    }
}

fn parse_if_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    let start = input.as_ptr() as usize - original_input.as_ptr() as usize;
    
    // 1. Consume whitespace before 'if'
    let (input, _) = whitespace_handler(input)?;
    let (input, is_if) = opt(tag("if"))(input)?;
    
    if is_if.is_some() {
        // 2. Whitespace after 'if'
        let (input, _) = whitespace_handler(input)?;
        let (input, condition) = parse_lambda_expr(original_input, input, ctx)?;
        eprintln!("After parsing condition, next chars: {:?}", &input[..input.len().min(20)]);

        // 3. Handle 'then' with whitespace around it
        let (input, _) = whitespace_handler(input)?;
        let (input, _) = tag("then")(input)?;
        let (input, _) = whitespace_handler(input)?;

        let (input, then_branch) = parse_if_expr(original_input, input, ctx)?;

        // 4. Handle 'else' with whitespace around it
        let (input, _) = whitespace_handler(input)?;
        let (input, _) = tag("else")(input)?;
        let (input, _) = whitespace_handler(input)?;

        let (input, else_branch) = parse_if_expr(original_input, input, ctx)?;
        
        let end = input.as_ptr() as usize - original_input.as_ptr() as usize;
        
        Ok((input, Expr::new(
            ExprKind::IfThenElse {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
            },
            calc_range(original_input, start, end - start)
        )))
    } else {
        parse_lambda_expr(original_input, input, ctx)
    }
}

fn parse_lambda_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    let start = input.as_ptr() as usize - original_input.as_ptr() as usize;
    
    // 1. Whitespace before pipe '|'
    let (input, _) = whitespace_handler(input)?;
    let (input, is_lambda) = opt(char('|'))(input)?;
    
    if is_lambda.is_some() {
        
        let (input, params) = separated_list0(
            |i| { 
                let (i, _) = whitespace_handler(i)?; 
                char(',')(i) 
            }, 
            |i| {
                let (i, _) = whitespace_handler(i)?;
                parse_non_keyword_identifier(i)
            }
        )(input)?;

        // 2. Handle closing pipe '|'
        let (input, _) = whitespace_handler(input)?;
        let (input, _) = char('|')(input)?;

        // 3. Handle arrow '->' with whitespace around it
        let (input, _) = whitespace_handler(input)?;
        let (input, _) = tag("->")(input)?;
        let (input, _) = whitespace_handler(input)?;
        
        let (input, body) = parse_expr_with_context(original_input, input, ctx)?;
        
        let end = input.as_ptr() as usize - original_input.as_ptr() as usize;
        
        Ok((input, Expr::new(
            ExprKind::Lambda {
                params: params.into_iter().map(|s| s.to_string()).collect(),
                body: Box::new(body),
            },
            calc_range(original_input, start, end - start)
        )))
    } else {
        parse_logical_or_expr(original_input, input, ctx)
    }
}

fn parse_logical_or_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    parse_binary_op(
        original_input,
        input,
        ctx,
        |o, i, c| parse_logical_and_expr(o, i, c),
        alt((value(BinaryOperator::Or, ws(tag("||"))),))
    )
}

fn parse_logical_and_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    parse_binary_op(
        original_input,
        input,
        ctx,
        |o, i, c| parse_comparison_expr(o, i, c),
        alt((value(BinaryOperator::And, ws(tag("&&"))),))
    )
}

fn parse_comparison_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    parse_binary_op(
        original_input,
        input,
        ctx,
        |o, i, c| parse_tensor_expr(o, i, c),
        alt((
            value(BinaryOperator::Eq, ws(tag("=="))),
            value(BinaryOperator::Ne, ws(tag("!="))),
            value(BinaryOperator::Le, ws(tag("<="))),
            value(BinaryOperator::Ge, ws(tag(">="))),
            value(BinaryOperator::Lt, ws(char('<'))),
            value(BinaryOperator::Gt, ws(char('>'))),
        ))
    )
}

fn parse_tensor_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    parse_binary_op(
        original_input,
        input,
        ctx,
        |o, i, c| parse_range_expr(o, i, c),
        alt((
            value(BinaryOperator::Tensor, ws(alt((tag("⊗"), tag("tensor"))))),
        ))
    )
}

fn parse_range_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    parse_binary_op(
        original_input,
        input,
        ctx,
        |o, i, c| parse_additive_expr(o, i, c),
        alt((value(BinaryOperator::Range, ws(tag(".."))),))
    )
}

fn parse_additive_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    parse_binary_op(
        original_input,
        input,
        ctx,
        |o, i, c| parse_multiplicative_expr(o, i, c),
        alt((
            value(BinaryOperator::Add, ws(char('+'))),
            value(BinaryOperator::Sub, ws(char('-'))),
        ))
    )
}

fn parse_multiplicative_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    parse_binary_op(
        original_input,
        input,
        ctx,
        |o, i, c| parse_unary_expr(o, i, c),
        alt((
            value(BinaryOperator::Mul, ws(char('*'))),
            value(BinaryOperator::Div, ws(char('/'))),
            value(BinaryOperator::Mod, ws(char('%'))),
        ))
    )
}

fn parse_binary_op<'a, F, G>(
    original_input: &'a str,
    input: &'a str,
    ctx: &mut ParseContext,
    mut next_level: F,
    mut op_parser: G,
) -> IResult<&'a str, Expr>
where
    F: FnMut(&'a str, &'a str, &mut ParseContext) -> IResult<&'a str, Expr>,
    G: FnMut(&'a str) -> IResult<&'a str, BinaryOperator>,
{
    let start = input.as_ptr() as usize - original_input.as_ptr() as usize;
    let (input, left) = next_level(original_input, input, ctx)?;
    
    let (input, ops_and_rights) = many0(pair(&mut op_parser, |i| next_level(original_input, i, ctx)))(input)?;
    
    if ops_and_rights.is_empty() {
        return Ok((input, left));
    }
    
    let mut result = left;
    let current_start = start;
    
    for (op, right) in ops_and_rights {
        let end = input.as_ptr() as usize - original_input.as_ptr() as usize;
        result = Expr::new(
            ExprKind::BinaryOp {
                op,
                left: Box::new(result),
                right: Box::new(right),
            },
            calc_range(original_input, current_start, end - current_start)
        );
    }
    
    Ok((input, result))
}

fn parse_unary_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    let start = input.as_ptr() as usize - original_input.as_ptr() as usize;

    let op_parse = alt((
        value(UnaryOperator::Not, ws(char('!'))),
        value(UnaryOperator::Neg, ws(char('-'))),
    ))(input);

    match op_parse {
        Ok((rest, op)) => {
            let (rest, operand) = parse_unary_expr(original_input, rest, ctx)?;
            let end = operand.range.end.character as usize;
            Ok((rest, Expr::new(
                ExprKind::UnaryOp {
                    op,
                    operand: Box::new(operand),
                },
                calc_range(original_input, start, end - start)
            )))
        },
        Err(_) => {
            parse_postfix_expr(original_input, input, ctx)
        }
    }
}

fn parse_postfix_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    let (mut current_input, mut base) = parse_primary_expr(original_input, input, ctx)?;
    let start = base.range.start.character as usize;

    loop {
        if let Ok((rest, _)) = ws(char('.'))(current_input) {
            // Tuple Projection / Dynamic Indexing with Parentheses
            if let Ok((rest_inner, _)) = tag::<_, _, Error<&str>>("(")(rest) {

                // Tuple Projection .(0)
                if let Ok((rest_idx, idx_str)) = terminated(digit1, ws(char(')')))(rest_inner) {
                    let idx = idx_str.parse::<usize>().unwrap_or(0);
                    let end = rest_idx.as_ptr() as usize - original_input.as_ptr() as usize;

                    base = Expr::new(
                        ExprKind::Projection {
                            index: idx,
                            tuple: Box::new(base),
                        },
                        calc_range(original_input, start, end - start)
                    );
                    current_input = rest_idx;
                    continue;
                }

                // Dynamic Indexing .(expr)
                let (rest_final, index_expr) = terminated(
                    |i| parse_expr_with_context(original_input, i, ctx),
                    ws(char(')'))
                )(rest_inner)?;

                let end = rest_final.as_ptr() as usize - original_input.as_ptr() as usize;

                base = Expr::new(
                    ExprKind::IndexAccess {
                        object: Box::new(base),
                        index: Box::new(index_expr),
                    },
                    calc_range(original_input, start, end - start)
                );
                current_input = rest_final;
                continue;
            }

            // Field access
            if let Ok((rest_inner, field)) = parse_identifier(rest) {
                let end = rest_inner.as_ptr() as usize - original_input.as_ptr() as usize;
                base = Expr::new(
                    ExprKind::FieldAccess {
                        object: Box::new(base),
                        field: field.to_string(),
                    },
                    calc_range(original_input, start, end - start)
                );
                current_input = rest_inner;
                continue;
            }
        }

        // Indexing
        if let Ok((rest, _)) = ws(char('['))(current_input) {
            let (rest, index_expr) = parse_expr_with_context(original_input, rest, ctx)?;
            let (rest, _) = ws(char(']'))(rest)?;

            let end = rest.as_ptr() as usize - original_input.as_ptr() as usize;
            base = Expr::new(
                ExprKind::IndexAccess {
                    object: Box::new(base),
                    index: Box::new(index_expr),
                },
                calc_range(original_input, start, end - start)
            );
            current_input = rest;
            continue;
        }

        // Function call
        if let Ok((rest, _)) = ws(char('('))(current_input) {
            let (rest, args) = separated_list0(
                ws(char(',')),
                |i| parse_expr_with_context(original_input, i, ctx)
            )(rest)?;
            let (rest, _) = ws(char(')'))(rest)?;

            let end = rest.as_ptr() as usize - original_input.as_ptr() as usize;
            base = Expr::new(
                ExprKind::FunctionCall {
                    function: Box::new(base),
                    args,
                },
                calc_range(original_input, start, end - start)
            );
            current_input = rest;
            continue;
        }

        break;
    }
    
    Ok((current_input, base))
}


fn parse_primary_expr<'a>(
    original_input: &'a str, 
    input: &'a str,
    ctx: &mut ParseContext
) -> IResult<&'a str, Expr> {
    let start: usize = input.as_ptr() as usize - original_input.as_ptr() as usize;

    if let Ok((rest, _)) = ws(tag("None"))(input) {
        return Ok((rest, Expr::new(ExprKind::None, calc_range(original_input, start, 4))));
    }
    if let Ok((rest, _)) = ws(tag("true"))(input) {
        return Ok((rest, Expr::bool(true, calc_range(original_input, start, 4))));
    }
    if let Ok((rest, _)) = ws(tag("false"))(input) {
        return Ok((rest, Expr::bool(false, calc_range(original_input, start, 5))));
    }
    if let Ok((rest, val)) = parse_number(original_input)(input) {
        return Ok((rest, val));
    }
    if let Ok((rest, val)) = parse_string_literal(original_input)(input) {
        return Ok((rest, val));
    }

    if let Ok((rest, _)) = ws(tag("Some"))(input) {
        let (rest, _) = ws(char('('))(rest)?;
        let (rest, expr) = parse_expr_with_context(original_input, rest, ctx)?;
        let (rest, _) = ws(char(')'))(rest)?;

        let end = rest.as_ptr() as usize - original_input.as_ptr() as usize;
        return Ok((rest, Expr::new(
            ExprKind::Some(Box::new(expr)),
            calc_range(original_input, start, end - start)
        )));
    }

    // List literal
    if let Ok((rest, _)) = ws(char('['))(input) {
        let (rest, exprs) = separated_list0(ws(char(',')), |i| parse_expr_with_context(original_input, i, ctx))(rest)?;
        let (rest, _) = ws(char(']'))(rest)?;

        let end = rest.as_ptr() as usize - original_input.as_ptr() as usize;
        return Ok((rest, Expr::new(
            ExprKind::List(exprs),
            calc_range(original_input, start, end - start)
        )));
    }

    // Tuple literal / Parenthesized expression
    if let Ok((rest, _)) = ws(char('('))(input) {
        let (rest, exprs) = separated_list0(ws(char(',')), |i| parse_expr_with_context(original_input, i, ctx))(rest)?;
        let (rest, _) = ws(char(')'))(rest)?;

        let end = rest.as_ptr() as usize - original_input.as_ptr() as usize;
        if exprs.len() == 1 {
            return Ok((rest, exprs.into_iter().next().unwrap()));
        } else {
            return Ok((rest, Expr::new(
                ExprKind::Tuple(exprs),
                calc_range(original_input, start, end - start)
            )));
        }
    }

    // Struct literal
    let (rest_after_id, id_str) = parse_identifier(input)?;
    if let Ok((_, _)) = peek(ws(char('{')))(rest_after_id) {
        if !is_keyword(id_str) {
            let (rest, _) = ws(char('{'))(rest_after_id)?;
            let (rest, fields) = separated_list0(
                ws(char(',')),
                map(
                    tuple((
                        parse_identifier,
                        ws(char('=')),
                        |i| parse_expr_with_context(original_input, i, ctx)
                    )),
                    |(name, _, expr)| (name.to_string(), expr)
                )
            )(rest)?;
            let (rest, _) = ws(char('}'))(rest)?;

            let end = rest.as_ptr() as usize - original_input.as_ptr() as usize;
            return Ok((rest, Expr::new(
                ExprKind::StructLiteral {
                    name: id_str.to_string(),
                    fields,
                },
                calc_range(original_input, start, end - start)
            )));
        }
    }

    let len = id_str.len();
    Ok((rest_after_id, Expr::identifier(id_str.to_string(), calc_range(original_input, start, len))))
}

fn parse_number<'a>(original_input: &'a str) -> impl FnMut(&'a str) -> IResult<&'a str, Expr> {
    move |input: &'a str| {
        let start = input.as_ptr() as usize - original_input.as_ptr() as usize;
        
        if let Ok((input, float_str)) = recognize::<_, _, Error<&str>, _>(tuple((
            opt(char('-')),
            digit1,
            alt((
                recognize(tuple((char('.'), digit1, opt(tuple((alt((char('e'), char('E'))), opt(alt((char('+'), char('-')))), digit1)))))),
                recognize(tuple((alt((char('e'), char('E'))), opt(alt((char('+'), char('-')))), digit1))),
            ))
        )))(input) {
            let len = float_str.len();
            if let Ok(value) = float_str.parse::<f64>() {
                return Ok((input, Expr::float(value, calc_range(original_input, start, len))));
            }
        }
        
        let (input, int_str) = recognize(tuple((opt(char('-')), digit1)))(input)?;
        let len = int_str.len();
        
        if let Ok(value) = int_str.parse::<i64>() {
            Ok((input, Expr::int(value, calc_range(original_input, start, len))))
        } else {
            Err(nom::Err::Error(Error::new(input, nom::error::ErrorKind::Digit)))
        }
    }
}

fn parse_string_literal<'a>(original_input: &'a str) -> impl FnMut(&'a str) -> IResult<&'a str, Expr> {
    move |input: &'a str| {
        let start = input.as_ptr() as usize - original_input.as_ptr() as usize;
        
        let (input, _) = char('\'')(input)?;
        let (input, content) = take_while(|c| c != '\'')(input)?;
        let (input, _) = char('\'')(input)?;
        
        let len = content.len() + 2;
        
        Ok((input, Expr::string(content.to_string(), calc_range(original_input, start, len))))
    }
}
