use std::{
    fmt::{Debug, Display},
    iter::zip,
};

use super::{Error, ErrorType, Exportable, Expr, ExprOps, ExprSet, ExprType, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Matcher {
    Alias(Box<Matcher>, String),
    DontCare,
    Ident(String),
    Tuple(Vec<Matcher>),
    Object(Vec<(String, Matcher)>, bool),
}

impl Display for Matcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self) // TODO: Don't use debug here
    }
}

impl Matcher {
    pub fn run<T, F>(&self, expr: Expr<T, F>) -> Result<ExprSet<T, F>, F>
    where
        T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
        F: Clone + Debug,
    {
        match self {
            Matcher::Alias(matcher, name) => {
                let mut output = matcher.run(expr.clone())?;
                // TODO: Check if overlapping keysets
                output.insert(name.clone(), expr);
                Ok(output)
            }
            Matcher::DontCare => Ok(ExprSet::new()),
            Matcher::Ident(name) => Ok(ExprSet::from([(name.to_string(), expr)])),
            Matcher::Tuple(matchers) => match &expr.res_type()?.tok {
                ExprType::Tuple(exprs) => {
                    if exprs.len() != matchers.len() {
                        Err(Error::new(
                            ErrorType::Type,
                            format!("Expected tuple of length {}", matchers.len()),
                        )
                        .reref(&expr.get_loc()))?;
                    }
                    let mut output = ExprSet::new();
                    for (itmatch, itexpr) in zip(matchers, exprs) {
                        let mut subvars = itmatch.run(itexpr.clone())?;
                        // TODO: Check if overlapping keysets
                        output.append(&mut subvars);
                    }
                    Ok(output)
                }
                _ => Err(Error::new(ErrorType::Type, "Expected tuple").reref(&expr.get_loc())),
            },
            Matcher::Object(items, need_all) => match &expr.res_type()?.tok {
                ExprType::Object(exprs) => {
                    let mut input = exprs.clone();
                    let mut output = ExprSet::new();

                    for (itname, itmatch) in items.iter() {
                        let in_expr = input.remove(itname).ok_or_else(|| {
                            Error::new(
                                ErrorType::NoValue,
                                format!("Expected field '{}' not found", itname),
                            )
                            .reref(&expr.get_loc())
                        })?;
                        let mut subvars = itmatch.run(in_expr.clone())?;
                        // TODO: Check if overlapping keysets
                        output.append(&mut subvars);
                    }

                    if *need_all && !input.is_empty() {
                        Err(
                            Error::new(ErrorType::NoValue, "Extra fields passed to function")
                                .reref(&expr.get_loc()),
                        )?
                    }

                    Ok(output)
                }
                _ => Err(Error::new(ErrorType::Type, "Expected tuple").reref(&expr.get_loc())),
            },
        }
    }
}
