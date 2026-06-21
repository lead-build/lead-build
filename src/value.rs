use std::{fmt::Display, rc::Rc};

use strum::EnumTryAs;

use crate::{
    lang::{Error, ErrorType, Exportable, ExprOps, ParsableValue, Result},
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
    BuildConcat(Vec<Value>),
}

impl Exportable for Value {
    fn export(&self, _indent: i32, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(v) => v.fmt(f),
            Value::String(v) => write!(f, "\"{}\"", v),
            Value::Path(v) => v.fmt(f),
            Value::Bool(v) => v.fmt(f),
            Value::Build(v) => v.fmt(f),
            Value::BuildRule(v) => v.fmt(f),
            Value::BuildVar(v) => write!(f, "${}", v),
            Value::BuildConcat(vs) => {
                for v in vs.iter() {
                    v.fmt(f)?;
                }
                Ok(())
            }
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.export(0, f)
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

impl<F> ExprOps<F> for Value
where
    F: Clone,
{
    fn op_add(lhs: &Self, rhs: &Self) -> Result<Self, F> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs + rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::String(lhs.clone() + rhs)),
            (Value::BuildConcat(vs), _) => {
                let mut vs = vs.clone();
                vs.push(rhs.clone());
                Ok(Value::BuildConcat(vs))
            }
            (_, _) => Ok(Value::BuildConcat(vec![lhs.clone(), rhs.clone()])),
        }
    }

    fn op_sub(lhs: &Self, rhs: &Self) -> Result<Self, F> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs - rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't subtract {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_mult(lhs: &Self, rhs: &Self) -> Result<Self, F> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs * rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't multiply {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_div(lhs: &Self, rhs: &Self) -> Result<Self, F> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Int(lhs / rhs)),
            (Value::Path(lhs), Value::String(rhs)) => match lhs.clone().step(rhs) {
                Some(path) => Ok(Value::Path(path)),
                None => todo!(),
            },
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't divide {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_lt(lhs: &Self, rhs: &Self) -> Result<Self, F> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs < rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs < rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't compare {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_le(lhs: &Self, rhs: &Self) -> Result<Self, F> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs <= rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs <= rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't compare {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_gt(lhs: &Self, rhs: &Self) -> Result<Self, F> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs > rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs > rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't compare {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_ge(lhs: &Self, rhs: &Self) -> Result<Self, F> {
        match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => Ok(Value::Bool(lhs >= rhs)),
            (Value::String(lhs), Value::String(rhs)) => Ok(Value::Bool(lhs >= rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't compare {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_eq(lhs: &Self, rhs: &Self) -> Result<Self, F> {
        Ok(Value::Bool(lhs == rhs))
    }

    fn op_neq(lhs: &Self, rhs: &Self) -> Result<Self, F> {
        Ok(Value::Bool(lhs != rhs))
    }

    fn op_neg(&self) -> Result<Self, F> {
        match self {
            Value::Int(val) => Ok(Value::Int(-val)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("not an integer: {}", self),
            )),
        }
    }

    fn op_not(&self) -> Result<Self, F> {
        match self {
            Value::Bool(val) => Ok(Value::Bool(!val)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("not a boolean: {}", self),
            )),
        }
    }

    fn as_bool(&self) -> Result<bool, F> {
        match self {
            Value::Bool(val) => Ok(*val),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("not a boolean: {}", self),
            )),
        }
    }

    fn as_string(&self) -> Result<String, F> {
        match self {
            Value::String(val) => Ok(val.clone()),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("not a string: {}", self),
            )),
        }
    }

    fn new_from_bool(&self, value: bool) -> Self {
        Value::Bool(value)
    }
}
