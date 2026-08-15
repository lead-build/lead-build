use std::{fmt::Display, rc::Rc};

use crate::{
    lang::{Error, ErrorType, Exportable, ExprOps, ParsableValue, Result},
    path::VirtPath,
    pbbuild::PbBuild,
};

#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Int(i64),
    String(String),
    Path {
        path: VirtPath,
        depends: Vec<Rc<PbBuild>>,
    },
    Bool(bool),

    BuildVar(String),
    BuildConcat(Vec<Value>),
}

impl Value {
    pub fn path(path: VirtPath) -> Self {
        Value::Path {
            path,
            depends: vec![],
        }
    }

    pub fn path_with_deps(path: VirtPath, deps: Vec<Rc<PbBuild>>) -> Self {
        Value::Path {
            path,
            depends: deps,
        }
    }

    pub fn try_as_path(self) -> Option<VirtPath> {
        match self {
            Value::Path { path, .. } => Some(path),
            _ => None,
        }
    }
}

impl Exportable for Value {
    fn export(&self, _indent: i32, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(v) => v.fmt(f),
            Value::String(v) => write!(f, "\"{}\"", v),
            Value::Path { path: v, .. } => v.fmt(f),
            Value::Bool(v) => v.fmt(f),
            Value::BuildVar(v) => write!(f, "${}", v),
            Value::BuildConcat(vs) => {
                for (i, v) in vs.iter().enumerate() {
                    if i != 0 {
                        write!(f, " + ")?;
                    }
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
            (Value::Path { path: lhs, depends }, Value::String(rhs)) => Ok(Value::Path {
                path: lhs.add_suffix(rhs)?,
                depends: depends.clone(),
            }),
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
            (Value::Path { path: lhs, depends }, Value::String(rhs)) => Ok(Value::Path {
                path: lhs.remove_suffix(rhs)?,
                depends: depends.clone(),
            }),
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
            (Value::Path { path: lhs, depends }, Value::String(rhs)) => Ok(Value::Path {
                path: lhs.clone().step(rhs)?,
                depends: depends.clone(),
            }),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't divide {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_string_concat(parts: Vec<Self>) -> Result<Self, F> {
        let mut parts_iter = parts.into_iter().peekable();
        let mut leading_path: Option<(VirtPath, Vec<Rc<PbBuild>>)> = None;
        let mut out: Vec<Self> = Vec::new();

        if matches!(parts_iter.peek(), Some(Value::Path { .. }))
            && let Some(Value::Path { path, depends }) = parts_iter.next()
        {
            leading_path = Some((path, depends));
        }

        for part in parts_iter {
            match (out.pop(), part) {
                (Some(Value::String(mut acc)), Value::String(value)) => {
                    acc.push_str(&value);
                    out.push(Value::String(acc));
                }
                (acc, Value::BuildConcat(mut parts)) => {
                    if let Some(acc_val) = acc {
                        out.push(acc_val);
                    }
                    out.append(&mut parts);
                }
                (acc, part) => {
                    if let Some(acc_val) = acc {
                        out.push(acc_val);
                    }
                    out.push(part);
                }
            }
        }

        if let Some((path, deps)) = leading_path
            && out.len() == 1
        {
            Ok(Value::Path {
                path: path.apply(out.pop().unwrap().as_string()?.as_str())?,
                depends: deps,
            })
        } else {
            if out.len() == 1 {
                Ok(out.pop().unwrap())
            } else {
                Ok(Value::BuildConcat(out))
            }
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

    fn new_from_bool(value: bool) -> Self {
        Value::Bool(value)
    }

    fn new_from_string(value: impl ToString) -> Self {
        Value::String(value.to_string())
    }
}
