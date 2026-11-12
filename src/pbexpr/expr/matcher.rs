use std::{
    fmt::{Debug, Display},
    iter::zip,
};

use super::{Error, ErrorType, Exportable, Expr, ExprOps, ExprSet, ExprType, Referrable, Result};
use crate::pbexpr::suggest::{closest, name_list};
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
    /// The `bool` is whether the list must have exactly this many elements
    /// (`[a, b]`), as opposed to at least this many (`[a, b, ...]`).
    List(Vec<Matcher<T, F>>, bool),
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
            Matcher::List(matchers, need_all) => Matcher::List(
                matchers
                    .iter()
                    .map(|matcher| matcher.bind_defaults(varscope))
                    .collect(),
                *need_all,
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
        // The value is pointed out where it is passed and where it was
        // written, which for a variable are two places
        let mismatch = |typ, msg: String| {
            Ok(Err(Error::new(typ, msg).reref(&expr.get_loc()).label(
                &expr.origin(),
                format!("this is {}", expr.describe()),
            )))
        };

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
                            format!(
                                "expected a tuple of {} items, got one of {}",
                                matchers.len(),
                                exprs.len()
                            ),
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
                other => mismatch(
                    ErrorType::Type,
                    format!("expected a tuple, got {}", other.describe()),
                ),
            },
            Matcher::List(matchers, need_all) => match &expr.res_type()?.tok {
                ExprType::List(exprs) => {
                    if *need_all && exprs.len() != matchers.len() {
                        return mismatch(
                            ErrorType::Type,
                            format!(
                                "expected a list of {} items, got one of {}",
                                matchers.len(),
                                exprs.len()
                            ),
                        );
                    }
                    if exprs.len() < matchers.len() {
                        return mismatch(
                            ErrorType::Type,
                            format!(
                                "expected a list of at least {} items, got one of {}",
                                matchers.len(),
                                exprs.len()
                            ),
                        );
                    }
                    let mut output = ExprSet::new();
                    // zip stops at the shorter one: elements past the
                    // matched prefix are left untouched (and unevaluated)
                    for (itmatch, itexpr) in zip(matchers, exprs) {
                        let mut subvars = sub_match!(itmatch, itexpr.clone());
                        // TODO: Check if overlapping keysets
                        output.append(&mut subvars);
                    }
                    Ok(Ok(output))
                }
                other => mismatch(
                    ErrorType::Type,
                    format!("expected a list, got {}", other.describe()),
                ),
            },
            Matcher::Object(items, need_all) => match &expr.res_type()?.tok {
                ExprType::Object(exprs) => {
                    let mut input = exprs.clone();
                    let mut output = ExprSet::new();

                    for (itname, itmatch, itdefault) in items.iter() {
                        let Some(in_expr) = input.remove(itname).or_else(|| itdefault.clone())
                        else {
                            // TODO: Add location of matcher
                            let given = exprs.keys().map(|key| key.as_string());
                            return Ok(mismatch(
                                ErrorType::NoValue,
                                format!("the field `{}` is missing", itname),
                            )?
                            .map_err(|err| {
                                match closest(&itname.as_string(), given) {
                                    Some(similar) => err.help(format!(
                                        "a field with a similar name is given: `{similar}`"
                                    )),
                                    None => err,
                                }
                            }));
                        };
                        let mut subvars = sub_match!(itmatch, in_expr);
                        // TODO: Check if overlapping keysets
                        output.append(&mut subvars);
                    }

                    if *need_all && !input.is_empty() {
                        let extra = name_list(input.keys().map(|key| key.as_string()));
                        let expected = name_list(items.iter().map(|(name, ..)| name.as_string()));
                        return Ok(mismatch(
                            ErrorType::NoValue,
                            if input.len() == 1 {
                                format!("the field {extra} is not expected")
                            } else {
                                format!("the fields {extra} are not expected")
                            },
                        )?
                        .map_err(|err| {
                            if items.is_empty() {
                                err.help("no fields are expected")
                            } else {
                                err.help(format!("the expected fields are {expected}"))
                            }
                        }));
                    }

                    Ok(Ok(output))
                }
                other => mismatch(
                    ErrorType::Type,
                    format!("expected an object, got {}", other.describe()),
                ),
            },
        }
    }
}
