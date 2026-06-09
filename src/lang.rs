mod error;
mod expr;
mod immap;
mod parser;
mod stringdecode;

#[cfg(test)]
mod testvalue;

pub use error::Result;
pub use expr::{Error as ExprError, Expr, ExprSet, ExprType};
pub use parser::{ParsableValue, parse_str};

pub mod ops {
    pub use super::expr::ops::{Error, ExprBuiltin, ExprOps, Result};
}
