use std::fmt::Display;

use strum::EnumTryAs;

use super::{
    expr::ops::{Error, ExprOps, Result},
    parser::ParsableValue,
};

#[derive(Clone, PartialEq, Debug, EnumTryAs)]
pub enum TestValue {
    Int(i64),
    String(String),
    Bool(bool),
}

impl Display for TestValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestValue::Int(v) => v.fmt(f),
            TestValue::String(v) => v.fmt(f),
            TestValue::Bool(v) => v.fmt(f),
        }
    }
}

impl ParsableValue for TestValue {
    fn parse_int(value: impl ToString) -> Option<Self> {
        Some(TestValue::Int(value.to_string().parse().unwrap()))
    }

    fn parse_string(value: impl ToString) -> Option<Self> {
        Some(TestValue::String(value.to_string()))
    }

    fn from_bool(value: bool) -> Self {
        TestValue::Bool(value)
    }
}

impl ExprOps for TestValue {
    fn op_add(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Int(lhs + rhs)),
            _ => Err(Error::Type(format!("can't add {} and {}", lhs, rhs))),
        }
    }

    fn op_sub(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Int(lhs - rhs)),
            _ => Err(Error::Type(format!("can't subtract {} and {}", lhs, rhs))),
        }
    }

    fn op_mult(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Int(lhs * rhs)),
            _ => Err(Error::Type(format!("can't multiply {} and {}", lhs, rhs))),
        }
    }

    fn op_div(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Int(lhs / rhs)),
            _ => Err(Error::Type(format!("can't divide {} and {}", lhs, rhs))),
        }
    }

    fn op_lt(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs < rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs < rhs)),
            _ => Err(Error::Type(format!("can't compare {} and {}", lhs, rhs))),
        }
    }

    fn op_le(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs <= rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs <= rhs)),
            _ => Err(Error::Type(format!("can't compare {} and {}", lhs, rhs))),
        }
    }

    fn op_gt(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs > rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs > rhs)),
            _ => Err(Error::Type(format!("can't compare {} and {}", lhs, rhs))),
        }
    }

    fn op_ge(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs >= rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs >= rhs)),
            _ => Err(Error::Type(format!("can't compare {} and {}", lhs, rhs))),
        }
    }

    fn op_eq(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs == rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs == rhs)),
            (TestValue::Bool(lhs), TestValue::Bool(rhs)) => Ok(TestValue::Bool(lhs == rhs)),
            _ => Ok(TestValue::Bool(false)),
        }
    }

    fn op_neq(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs != rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs != rhs)),
            (TestValue::Bool(lhs), TestValue::Bool(rhs)) => Ok(TestValue::Bool(lhs != rhs)),
            _ => Ok(TestValue::Bool(true)),
        }
    }

    fn op_neg(&self) -> Result<Self> {
        match self {
            TestValue::Int(val) => Ok(TestValue::Int(-val)),
            _ => Err(Error::Type(format!("not an integer: {}", self))),
        }
    }

    fn op_not(&self) -> Result<Self> {
        match self {
            TestValue::Bool(val) => Ok(TestValue::Bool(!val)),
            _ => Err(Error::Type(format!("not a boolean: {}", self))),
        }
    }

    fn as_bool(&self) -> Result<bool> {
        match self {
            TestValue::Bool(val) => Ok(*val),
            _ => Err(Error::Type(format!("not a boolean: {}", self))),
        }
    }

    fn new_from_bool(&self, value: bool) -> Self {
        TestValue::Bool(value)
    }
}
