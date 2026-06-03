use std::fmt::Display;

use strum::EnumTryAs;

use crate::{
    expr::ops::{Error, ExprOps, Result},
    parser::ParsableValue,
};

#[derive(Clone, PartialEq, Debug, EnumTryAs)]
pub enum Value {
    Int(i64),
    String(String),
    Bool(bool),
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(v) => v.fmt(f),
            Value::String(v) => v.fmt(f),
            Value::Bool(v) => v.fmt(f),
        }
    }
}

impl ParsableValue for Value {
    fn parse_int(value: impl ToString) -> Option<Self> {
        Some(Value::Int(value.to_string().parse().unwrap()))
    }

    fn parse_string(value: impl ToString) -> Option<Self> {
        Some(Value::String(value.to_string()))
    }
}

impl ExprOps for Value {
    fn op_add(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs + rhs)),
            _ => Err(Error::Type(format!("cant add {} and {}", lhs, rhs))),
        }
    }

    fn op_sub(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs - rhs)),
            _ => Err(Error::Type(format!("cant subtract {} and {}", lhs, rhs))),
        }
    }

    fn op_mult(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs * rhs)),
            _ => Err(Error::Type(format!("cant multiply {} and {}", lhs, rhs))),
        }
    }

    fn op_div(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs / rhs)),
            _ => Err(Error::Type(format!("cant divide {} and {}", lhs, rhs))),
        }
    }

    fn op_lt(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs < rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs < rhs)),
            _ => Err(Error::Type(format!("cant compare {} and {}", lhs, rhs))),
        }
    }

    fn op_le(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs <= rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs <= rhs)),
            _ => Err(Error::Type(format!("cant compare {} and {}", lhs, rhs))),
        }
    }

    fn op_gt(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs > rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs > rhs)),
            _ => Err(Error::Type(format!("cant compare {} and {}", lhs, rhs))),
        }
    }

    fn op_ge(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs >= rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs >= rhs)),
            _ => Err(Error::Type(format!("cant compare {} and {}", lhs, rhs))),
        }
    }

    fn op_eq(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs == rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs == rhs)),
            (Value::Bool(lhs), Value::Bool(rhs)) => Ok(Value::Bool(lhs == rhs)),
            _ => Ok(Value::Bool(false)),
        }
    }

    fn op_neq(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs != rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs != rhs)),
            (Value::Bool(lhs), Value::Bool(rhs)) => Ok(Value::Bool(lhs != rhs)),
            _ => Ok(Value::Bool(true)),
        }
    }

    fn op_neg(&self) -> Result<Self> {
        match self {
            Value::Int(val) => Ok(Value::Int(-val)),
            _ => Err(Error::Type(format!("not an integer: {}", self))),
        }
    }

    fn op_not(&self) -> Result<Self> {
        match self {
            Value::Bool(val) => Ok(Value::Bool(!val)),
            _ => Err(Error::Type(format!("not a boolean: {}", self))),
        }
    }

    fn as_bool(&self) -> Result<bool> {
        match self {
            Value::Bool(val) => Ok(*val),
            _ => Err(Error::Type(format!("not a boolean: {}", self))),
        }
    }

    fn from_bool(&self, value: bool) -> Self {
        Value::Bool(value)
    }
}
