use std::{
    collections::HashSet,
    fmt::{Debug, Display},
    iter::zip,
};

use super::{Error, ErrorType, Exportable, Expr, ExprOps, ExprSet, ExprType, Result};

pub type ObjectMatch<T, F> = (String, Matcher<T, F>, Option<Expr<T, F>>);

#[derive(Debug, Clone, PartialEq)]
pub enum Matcher<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    Alias(Box<Matcher<T, F>>, String),
    DontCare,
    Ident(String),
    Tuple(Vec<Matcher<T, F>>),
    Object(Vec<ObjectMatch<T, F>>, bool),
}

impl<T, F> Display for Matcher<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.export(0, f)
    }
}

impl<T, F> Exportable for Matcher<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug,
{
    fn export(&self, indent: i32, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Matcher::Alias(matcher, name) => {
                write!(f, "{} = ", name)?;
                matcher.export(indent, f)?;
                Ok(())
            }
            Matcher::DontCare => write!(f, "_"),
            Matcher::Ident(name) => write!(f, "{}", name),
            Matcher::Tuple(matchers) => {
                write!(f, "(")?;
                for (i, matcher) in matchers.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    matcher.export(indent, f)?;
                }
                write!(f, ")")
            }
            Matcher::Object(items, need_all) => {
                write!(f, "{{")?;
                for (i, (name, matcher, default)) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{} = ", name)?;
                    matcher.export(indent, f)?;
                    if let Some(default) = default {
                        write!(f, " ? ")?;
                        default.export(indent, f)?;
                    }
                }
                if *need_all {
                    write!(f, ", ...")?;
                }
                write!(f, "}}")
            }
        }
    }
}

impl<T, F> Matcher<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug,
{
    pub fn bind_defaults(&self, varscope: &ExprSet<T, F>) -> Matcher<T, F> {
        match self {
            Matcher::Alias(matcher, name) => {
                Matcher::Alias(Box::new(matcher.bind_defaults(varscope)), name.clone())
            }
            Matcher::DontCare => Matcher::DontCare,
            Matcher::Ident(name) => Matcher::Ident(name.clone()),
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
                            name.clone(),
                            matcher.bind_defaults(varscope),
                            default
                                .as_ref()
                                .map(|default_expr| default_expr.bind(varscope.clone())),
                        )
                    })
                    .collect(),
                *need_all,
            ),
        }
    }

    pub fn referenced_vars(&self) -> HashSet<String> {
        let mut referenced_vars = HashSet::new();
        self.visit_referenced_vars(&mut referenced_vars);
        referenced_vars
    }

    fn visit_referenced_vars(&self, referenced_vars: &mut HashSet<String>) {
        match self {
            Matcher::Alias(inner, name) => {
                let _ = name;
                inner.visit_referenced_vars(referenced_vars);
            }
            Matcher::DontCare => {}
            Matcher::Ident(_) => {}
            Matcher::Tuple(matchers) => {
                for item in matchers.iter() {
                    item.visit_referenced_vars(referenced_vars);
                }
            }
            Matcher::Object(items, _) => {
                for (_, item_matcher, item_default) in items.iter() {
                    if let Some(default_expr) = item_default {
                        referenced_vars.extend(default_expr.referenced_vars());
                    }
                    item_matcher.visit_referenced_vars(referenced_vars);
                }
            }
        }
    }

    pub fn run(&self, expr: Expr<T, F>) -> Result<ExprSet<T, F>, F>
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
