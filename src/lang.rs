mod error;
mod expr;
mod parser;
mod stringdecode;

#[cfg(test)]
mod testvalue;

pub use error::{Error, Result};
pub use expr::{Expr, ExprSet, ExprType};
pub use parser::{ParsableValue, parse_str};

pub mod ops {
    pub use super::expr::ops::{Error, ExprBuiltin, ExprOps, Result};
}
