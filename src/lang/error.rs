use std::{
    fmt::{Debug, Display},
    result,
};

pub type Result<T, F> = result::Result<T, Error<F>>;

pub trait Referrable {
    fn format_ref(
        &self,
        left: usize,
        right: usize,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result;
}

#[derive(Debug)]
pub enum ErrorType {
    Parse,
    Scope,
    Eval,
    Type,
    DupKey,
    NoValue,
    Custom,
}

impl Display for ErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorType::Parse => write!(f, "Parse error: "),
            ErrorType::Scope => write!(f, "Scope error: "),
            ErrorType::Eval => write!(f, "Eval error: "),
            ErrorType::Type => write!(f, "Type error: "),
            ErrorType::DupKey => write!(f, "Duplicate key: "),
            ErrorType::NoValue => write!(f, "No value: "),
            ErrorType::Custom => Ok(()),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Loc<F> {
    pub file: F,
    pub left: usize,
    pub right: usize,
}

impl<F> Display for Loc<F>
where
    F: Referrable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.file.format_ref(self.left, self.right, f)
    }
}

#[derive(Debug)]
pub struct Error<F> {
    pub typ: ErrorType,
    pub msg: String,
    pub locs: Vec<Loc<F>>,
}

impl<F> std::error::Error for Error<F> where F: Debug + Referrable {}

impl<F> Display for Error<F>
where
    F: Referrable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}{}", self.typ, self.msg)?;
        if !self.locs.is_empty() {
            writeln!(f)?;
            writeln!(f, "Backtrace:")?;
            for (idx, loc) in self.locs.iter().enumerate() {
                writeln!(f, "  {:3} - {}", idx + 1, loc)?;
            }
        }
        Ok(())
    }
}

impl<F> Error<F>
where
    F: Clone,
{
    pub fn new(typ: ErrorType, msg: impl ToString) -> Error<F> {
        Error {
            typ,
            msg: msg.to_string(),
            locs: vec![],
        }
    }

    pub fn loc(self, left: usize, right: usize, file: &F) -> Self {
        let mut out = self;
        out.locs.push(Loc {
            left,
            right,
            file: file.clone(),
        });
        out
    }

    pub fn reref(self, loc: &Option<Loc<F>>) -> Self {
        let mut out = self;
        if let Some(loc) = loc {
            out.locs.push(loc.clone());
        }
        out
    }
}
