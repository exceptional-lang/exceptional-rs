use std::collections::BTreeMap;
use num::rational::{BigRational};

#[derive(Clone, Eq, Debug, Hash, Ord, PartialEq, PartialOrd)]
pub enum Value {
    // Nil,
    Number(BigRational),
    CharString(String),
    Boolean(bool),
    // Vec(Vec<Value>),
    // Map(BTreeMap<Rc<Value>, Rc<Value>>),
    Fn(Box<(Vec<(String)>, Vec<Statement>)>),
}

#[derive(Clone, Eq, Debug, Hash, Ord, PartialEq, PartialOrd)]
pub enum Statement {
    Assign(bool, String, Box<Expression>),
}

#[derive(Clone, Eq, Debug, Hash, Ord, PartialEq, PartialOrd)]
pub enum Expression {
    BinOp(String, Box<Expression>, Box<Expression>),
    Value(Value),
}