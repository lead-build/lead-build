use std::fmt::Display;

use strum::EnumTryAs;

use super::{
    Exportable,
    error::{Error, ErrorType, Referrable, Result},
    expr::ExprOps,
    parser::ParsableValue,
};

#[derive(Clone, Debug, PartialEq)]
pub struct FRef;

impl Referrable for FRef {
    fn format_ref(
        &self,
        left: usize,
        right: usize,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        write!(f, "FRef({},{})", left, right)
    }
}

#[derive(Clone, PartialEq, Debug, EnumTryAs)]
pub enum TestValue {
    Int(i64),
    String(String),
    Bool(bool),
}

impl Exportable for TestValue {
    fn export(&self) -> crate::pbexpr::ExportResult<crate::pbcst::PbNode> {
        match self {
            TestValue::Int(v) => Ok(crate::pbcst::PbNode::generated(
                crate::pbcst::PbNodeKind::Int(v.to_string()),
            )),
            TestValue::String(v) => Ok(crate::pbcst::PbNode::generated(
                crate::pbcst::PbNodeKind::String(format!("\"{v}\"")),
            )),
            TestValue::Bool(v) => Ok(crate::pbcst::PbNode::generated(
                crate::pbcst::PbNodeKind::Bool(*v),
            )),
        }
    }
}

impl Display for TestValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::pbcst::export::Exportable as PbExportable;
        self.export().unwrap().to_source().fmt(f)
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

impl ExprOps<FRef> for TestValue {
    fn op_add(lhs: &Self, rhs: &Self) -> Result<Self, FRef> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Int(lhs + rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => {
                Ok(TestValue::String(lhs.to_string() + rhs))
            }
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't add {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_sub(lhs: &Self, rhs: &Self) -> Result<Self, FRef> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Int(lhs - rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't subtract {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_mult(lhs: &Self, rhs: &Self) -> Result<Self, FRef> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Int(lhs * rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't multiply {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_div(lhs: &Self, rhs: &Self) -> Result<Self, FRef> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Int(lhs / rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't divide {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_string_concat(parts: Vec<Self>) -> Result<Self, FRef> {
        parts
            .into_iter()
            .try_fold(Self::String("".to_string()), |acc, part| {
                match (acc, part) {
                    (TestValue::String(lhs), TestValue::String(rhs)) => {
                        Ok(TestValue::String(lhs + &rhs))
                    }
                    (_, _) => Err(Error::new(
                        ErrorType::Type,
                        "can't concatenate non-string value".to_string(),
                    )),
                }
            })
    }

    fn op_lt(lhs: &Self, rhs: &Self) -> Result<Self, FRef> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs < rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs < rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't compare {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_le(lhs: &Self, rhs: &Self) -> Result<Self, FRef> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs <= rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs <= rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't compare {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_gt(lhs: &Self, rhs: &Self) -> Result<Self, FRef> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs > rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs > rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't compare {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_ge(lhs: &Self, rhs: &Self) -> Result<Self, FRef> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs >= rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs >= rhs)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("can't compare {} and {}", lhs, rhs),
            )),
        }
    }

    fn op_eq(lhs: &Self, rhs: &Self) -> Result<Self, FRef> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs == rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs == rhs)),
            (TestValue::Bool(lhs), TestValue::Bool(rhs)) => Ok(TestValue::Bool(lhs == rhs)),
            _ => Ok(TestValue::Bool(false)),
        }
    }

    fn op_neq(lhs: &Self, rhs: &Self) -> Result<Self, FRef> {
        match (lhs, rhs) {
            (TestValue::Int(lhs), TestValue::Int(rhs)) => Ok(TestValue::Bool(lhs != rhs)),
            (TestValue::String(lhs), TestValue::String(rhs)) => Ok(TestValue::Bool(lhs != rhs)),
            (TestValue::Bool(lhs), TestValue::Bool(rhs)) => Ok(TestValue::Bool(lhs != rhs)),
            _ => Ok(TestValue::Bool(true)),
        }
    }

    fn op_neg(&self) -> Result<Self, FRef> {
        match self {
            TestValue::Int(val) => Ok(TestValue::Int(-val)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("not an integer: {}", self),
            )),
        }
    }

    fn op_not(&self) -> Result<Self, FRef> {
        match self {
            TestValue::Bool(val) => Ok(TestValue::Bool(!val)),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("not a boolean: {}", self),
            )),
        }
    }

    fn as_bool(&self) -> Result<bool, FRef> {
        match self {
            TestValue::Bool(val) => Ok(*val),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("not a boolean: {}", self),
            )),
        }
    }

    fn as_string(&self) -> Result<String, FRef> {
        match self {
            TestValue::String(val) => Ok(val.clone()),
            _ => Err(Error::new(
                ErrorType::Type,
                format!("not a string: {}", self),
            )),
        }
    }

    fn new_from_bool(value: bool) -> Self {
        TestValue::Bool(value)
    }

    fn new_from_string(value: impl ToString) -> Self {
        TestValue::String(value.to_string())
    }
}
