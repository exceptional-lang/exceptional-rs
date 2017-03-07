#[cfg(test)]

use ast::*;
use num::rational::Ratio;
use num::BigInt;
use grammar::*;
use vm::*;
use std::rc::*;
use std::collections::BTreeMap;

pub fn l_string(string: &str) -> Literal {
    Literal::CharString(string.to_owned())
}

pub fn l_number(num: i64, denom: i64) -> Literal {
    Literal::Number(build_ratio(num, denom))
}

pub fn l_bool(b: bool) -> Literal {
    match b {
        true => Literal::Boolean(true),
        false => Literal::Boolean(false),
    }
}

pub fn l_function(args: Vec<String>, statements: Vec<Statement>) -> Literal {
    Literal::Fn(Box::new(args), Box::new(statements))
}

pub fn l_map(pairs: Vec<(Expression, Expression)>) -> Literal {
    Literal::Map(pairs)
}

pub fn p_map(pairs: Vec<(Pattern, Pattern)>) -> Pattern {
    Pattern::Map(pairs)
}

pub fn p_bool(b: bool) -> Pattern {
    Pattern::Boolean(b)
}

pub fn p_number(num: i64, denom: i64) -> Pattern {
    Pattern::Number(build_ratio(num, denom))
}

pub fn p_string(string: &str) -> Pattern {
    Pattern::CharString(string.to_owned())
}

pub fn p_ident(name: &str) -> Pattern {
    Pattern::Identifier(name.to_owned())
}

pub fn s_assign(name: &str, literal: Literal) -> Statement {
    Statement::Assign(true, name.to_owned(), Box::new(e_literal(literal)))
}

pub fn s_call(name: &str, args: Vec<Expression>) -> Statement {
    Statement::Call(name.to_owned(), args)
}

pub fn s_raise(exp: Expression) -> Statement {
    Statement::Raise(exp)
}

pub fn s_rescue(map: Pattern, statements: Vec<Statement>) -> Statement {
    Statement::Rescue(map, Box::new(statements))
}

pub fn e_literal(literal: Literal) -> Expression {
    Expression::Literal(literal)
}

pub fn e_identifier(name: &str) -> Expression {
    Expression::Identifier(name.to_owned())
}

pub fn e_binop(op: &str, left: Expression, right: Expression) -> Expression {
    Expression::BinOp(op.to_owned(), Box::new(left), Box::new(right))
}

pub fn build_ratio(num: i64, denom: i64) -> Ratio<BigInt> {
    Ratio::new(BigInt::from(num), BigInt::from(denom))
}

pub fn parse_expression(input: &str) -> Expression {
    expression(input).unwrap()
}

pub fn parse_statements(input: &str) -> Vec<Statement> {
    statements(input).unwrap()
}

pub fn parse_literal(input: &str) -> Literal {
    literal(input).unwrap()
}

pub fn v_bool(bool: bool) -> Value {
    Value::Boolean(bool)
}

pub fn v_string(str: &str) -> Value {
    Value::CharString(str.to_owned())
}

pub fn v_number(num: i64, denom: i64) -> Value {
    Value::Number(build_ratio(num, denom))
}

pub fn v_map(pairs: Vec<(Value, Value)>) -> Value {
    let map: BTreeMap<_, _> = pairs.into_iter()
        .map(|(key, value)| (Rc::new(key), Rc::new(value)))
        .collect::<BTreeMap<_, _>>();
    Value::Map(map)
}

macro_rules! assert_err {
    ($e:expr) => {
        match $e {
            Err(_) => {}
            res => {
                panic!("assertion failed: expected Err, got: {:?}", res)
            }
        }
    }
}