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

    pub fn run(&self, expr: Expr<T, F>) -> Result<ExprSet<T, F>, F> {
        self.run_checked(expr)?
    }

    /// Like [`Matcher::run`], but a value that simply doesn't have the
    /// matcher's shape is `Ok(None)` rather than an error — what `match`
    /// needs to fall through to its next case. Errors from evaluating the
    /// value itself still propagate.
    pub fn try_run(&self, expr: Expr<T, F>) -> Result<Option<ExprSet<T, F>>, F> {
        Ok(self.run_checked(expr)?.ok())
    }

    /// The outer `Result` is a failure to evaluate `expr` far enough to
    /// know its shape; the inner one is `expr` not fitting this matcher.
    fn run_checked(&self, expr: Expr<T, F>) -> Result<Result<ExprSet<T, F>, F>, F> {
        macro_rules! sub_match {
            ($matcher:expr, $expr:expr) => {
                match $matcher.run_checked($expr)? {
                    Ok(vars) => vars,
                    Err(mismatch) => return Ok(Err(mismatch)),
                }
            };
        }
        let mismatch = |typ, msg: String| Ok(Err(Error::new(typ, msg).reref(&expr.get_loc())));

        match self {
            Matcher::Alias(matcher, name) => {
                let mut output = sub_match!(matcher, expr.clone());
                // TODO: Check if overlapping keysets
                output.insert(*name, expr);
                Ok(Ok(output))
            }
            Matcher::DontCare => Ok(Ok(ExprSet::new())),
            Matcher::Ident(name) => Ok(Ok(ExprSet::from([(*name, expr)]))),
            Matcher::Tuple(matchers) => match &expr.res_type()?.tok {
                ExprType::Tuple(exprs) => {
                    if exprs.len() != matchers.len() {
                        return mismatch(
                            ErrorType::Type,
                            format!("Expected tuple of length {}", matchers.len()),
                        );
                    }
                    let mut output = ExprSet::new();
                    for (itmatch, itexpr) in zip(matchers, exprs) {
                        let mut subvars = sub_match!(itmatch, itexpr.clone());
                        // TODO: Check if overlapping keysets
                        output.append(&mut subvars);
                    }
                    Ok(Ok(output))
                }
                _ => mismatch(ErrorType::Type, "Expected tuple".to_string()),
            },
            Matcher::Object(items, need_all) => match &expr.res_type()?.tok {
                ExprType::Object(exprs) => {
                    let mut input = exprs.clone();
                    let mut output = ExprSet::new();

                    for (itname, itmatch, itdefault) in items.iter() {
                        let Some(in_expr) = input.remove(itname).or_else(|| itdefault.clone())
                        else {
                            // TODO: Add location of matcher
                            return mismatch(
                                ErrorType::NoValue,
                                format!("Expected field '{}' not found", itname),
                            );
                        };
                        let mut subvars = sub_match!(itmatch, in_expr);
                        // TODO: Check if overlapping keysets
                        output.append(&mut subvars);
                    }

                    if *need_all && !input.is_empty() {
                        return mismatch(
                            ErrorType::NoValue,
                            "Extra fields passed to function".to_string(),
                        );
                    }

                    Ok(Ok(output))
                }
                _ => mismatch(ErrorType::Type, "Expected tuple".to_string()),
            },
        }
    }
}
