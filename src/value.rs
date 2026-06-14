use std::{fmt::Display, rc::Rc};

use strum::EnumTryAs;

use crate::{
    lang::{
        ParsableValue,
        ops::{Error, ExprOps, Result},
    },
    path::VirtPath,
    pbbuild::{PbBuild, PbBuildRule},
};

#[derive(Clone, PartialEq, Debug, EnumTryAs)]
pub enum Value {
    Int(i64),
    String(String),
    Path(VirtPath),
    Bool(bool),

    Build(Rc<PbBuild>),
    BuildRule(Rc<PbBuildRule>),
    BuildVar(String),
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(v) => v.fmt(f),
            Value::String(v) => write!(f, "\"{}\"", v),
            Value::Path(v) => v.fmt(f),
            Value::Bool(v) => v.fmt(f),
            Value::Build(v) => v.fmt(f),
            Value::BuildRule(v) => v.fmt(f),
            Value::BuildVar(v) => write!(f, "${}", v),
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

    fn from_bool(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl ExprOps for Value {
    fn op_add(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs + rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::String(lhs.clone() + rhs)),
            (Value::Path(lhs), Value::String(rhs)) => Err(Error::Type(format!(
                "Can't use paths as part of strings (yet?) ({} + {})",
                lhs, rhs
            ))),
            _ => Err(Error::Type(format!("can't add {} and {}", lhs, rhs))),
        }
    }

    fn op_sub(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs - rhs)),
            _ => Err(Error::Type(format!("can't subtract {} and {}", lhs, rhs))),
        }
    }

    fn op_mult(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs * rhs)),
            _ => Err(Error::Type(format!("can't multiply {} and {}", lhs, rhs))),
        }
    }

    fn op_div(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs / rhs)),
            (Value::Path(lhs), Value::String(rhs)) => match lhs.clone().step(rhs) {
                Some(path) => Ok(Value::Path(path)),
                None => todo!(),
            },
            _ => Err(Error::Type(format!("can't divide {} and {}", lhs, rhs))),
        }
    }

    fn op_lt(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs < rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs < rhs)),
            _ => Err(Error::Type(format!("can't compare {} and {}", lhs, rhs))),
        }
    }

    fn op_le(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs <= rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs <= rhs)),
            _ => Err(Error::Type(format!("can't compare {} and {}", lhs, rhs))),
        }
    }

    fn op_gt(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs > rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs > rhs)),
            _ => Err(Error::Type(format!("can't compare {} and {}", lhs, rhs))),
        }
    }

    fn op_ge(lhs: &Self, rhs: &Self) -> Result<Self> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs >= rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs >= rhs)),
            _ => Err(Error::Type(format!("can't compare {} and {}", lhs, rhs))),
        }
    }

    fn op_eq(lhs: &Self, rhs: &Self) -> Result<Self> {
        Ok(Value::Bool(lhs == rhs))
    }

    fn op_neq(lhs: &Self, rhs: &Self) -> Result<Self> {
        Ok(Value::Bool(lhs != rhs))
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

    fn as_string(&self) -> Result<String> {
        match self {
            Value::String(val) => Ok(val.clone()),
            _ => Err(Error::Type(format!("not a string: {}", self))),
        }
    }

    fn new_from_bool(&self, value: bool) -> Self {
        Value::Bool(value)
    }
}
