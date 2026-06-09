mod error;
mod expr;
mod immap;
mod parser;
mod stringdecode;

#[cfg(test)]
mod testvalue;

pub use error::Result;
pub use expr::{Expr, ExprSet, ExprType};
pub use parser::{ParsableValue, Parser};

pub mod ops {
    pub use super::expr::ops::{Error, ExprOps, Result};
}
