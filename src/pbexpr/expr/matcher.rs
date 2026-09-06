use std::{
    fmt::{Debug, Display},
    iter::zip,
};

use super::{Error, ErrorType, Exportable, Expr, ExprOps, ExprSet, ExprType, Referrable, Result};
use crate::strkey::StrKey;

pub type ObjectMatch<T, F> = (StrKey, Matcher<T, F>, Option<Expr<T, F>>);

#[derive(Debug, Clone, PartialEq)]
pub enum Matcher<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    Alias(Box<Matcher<T, F>>, StrKey),
    DontCare,
    Ident(StrKey),
    Tuple(Vec<Matcher<T, F>>),
    Object(Vec<ObjectMatch<T, F>>, bool),
}

impl<T, F> Display for Matcher<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<semantic matcher>")
    }
}

impl<T, F> Matcher<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    pub fn bind_defaults(&self, varscope: &ExprSet<T, F>) -> Matcher<T, F> {
        match self {
            Matcher::Alias(matcher, name) => {
                Matcher::Alias(Box::new(matcher.bind_defaults(varscope)), *name)
            }
            Matcher::DontCare => Matcher::DontCare,
            Matcher::Ident(name) => Matcher::Ident(*name),
            Matcher::Tuple(matchers) => Matcher::Tuple(
                matchers
                    .iter()
                    .map(|matcher| matcher.bind_defaults(varscope))
                    .collect(),
            ),
            Matcher::Object(items, need_all) => Matcher::Object(
                items
                    .iter()
                    .map(|(name, matcher, default)| {
                        (
                            *name,
                            matcher.bind_defaults(varscope),
                            default
                                .as_ref()
                                .map(|default_expr| default_expr.bind(varscope)),
                        )
                    })
                    .collect(),
                *need_all,
            ),
        }
    }

    pub fn run(&self, expr: Expr<T, F>) -> Result<ExprSet<T, F>, F>
    where
        T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
        F: Clone + Debug + Referrable,
    {
        match self {
            Matcher::Alias(matcher, name) => {
                let mut output = matcher.run(expr.clone())?;
                // TODO: Check if overlapping keysets
                output.insert(*name, expr);
                Ok(output)
            }
            Matcher::DontCare => Ok(ExprSet::new()),
            Matcher::Ident(name) => Ok(ExprSet::from([(*name, expr)])),
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

                    for (itname, itmatch, itdefault) in items.iter() {
                        let in_expr = input
                            .remove(itname)
                            .or_else(|| itdefault.clone())
                            .ok_or_else(|| {
                                Error::new(
                                    ErrorType::NoValue,
                                    format!("Expected field '{}' not found", itname),
                                )
                                .reref(&expr.get_loc()) // TODO: Add location of matcher
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
