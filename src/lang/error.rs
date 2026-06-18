use std::result;

use super::{expr, parser};

pub type Result<T> = result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(#[from] parser::Error),

    #[error("Expression error: {0}")]
    Expr(#[from] expr::Error),

    #[error("{0}")]
    Custom(String),
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Error::Custom(value)
    }
}
