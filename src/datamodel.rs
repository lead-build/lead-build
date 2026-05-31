use clap::builder::FalseyValueParser;

use crate::{datamodel::Error::ResolvError, immap::ImMap};
use std::rc::Rc;

#[derive(Debug, PartialEq)]
pub enum Error {
    ResolvError(String),
    DupKey(String),
}

impl From<crate::immap::Error> for Error {
    fn from(value: crate::immap::Error) -> Self {
        match value {
            crate::immap::Error::DupKey(key) => Error::DupKey(key),
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, PartialEq)]
pub enum Expr {
    Object(ImMap<Rc<Expr>>),
    Int(i64),
    String(String),
    Var(String),
    FuncDefIdent(String, Rc<Expr>),
    FuncDefPattern(Vec<String>, Rc<Expr>),
    Let(Vec<(String, Rc<Expr>)>, Rc<Expr>),
    FuncCall(String, Rc<Expr>),
    BoundExpr(Scope, Rc<Expr>),
}

#[derive(Debug, Clone)]
pub struct Scope {
    vars: ImMap<Rc<Expr>>,
}

impl PartialEq for Scope {
    // PartialEq for scope should never be called. It needs to be avaialble for
    // PartialEq for Expr to be availble, which is only needed for tests
    fn eq(&self, _other: &Self) -> bool {
        unimplemented!("PartialEq for Scope should not be called")
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self { vars: ImMap::new() }
    }
}

impl Scope {
    fn resolve_once(&self, expr: Rc<Expr>) -> Result<Rc<Expr>> {
        match expr.as_ref() {
            Expr::Let(fields, target_expr) => {
                let mut vars: ImMap<Rc<Expr>> = self.vars.clone();
                for (field_name, field_expr) in fields {
                    vars = vars.set_inplace(field_name.clone(), field_expr.clone())?;
                }
                Ok(Expr::BoundExpr(Scope { vars }, target_expr.clone()).into())
            }
            Expr::BoundExpr(scope, subexpr) => match subexpr.as_ref() {
                Expr::Object(im_map) => Ok(Expr::Object(
                    im_map.map(|val| Expr::BoundExpr(scope.clone(), val.clone()).into()),
                )
                .into()),
                Expr::Var(name) => Ok(scope.vars.get(name).unwrap()),
                Expr::FuncDefIdent(_, expr) => todo!(),
                Expr::FuncDefPattern(items, expr) => todo!(),
                Expr::Let(items, expr) => todo!(),
                Expr::FuncCall(_, expr) => todo!(),
                Expr::BoundExpr(scope, expr) => todo!(),
                _ => Ok(subexpr.clone()),
            },
            Expr::Var(name) => match self.vars.get(name) {
                Some(value) => Ok(value),
                None => Err(Error::ResolvError("Unknown variable".into())),
            },
            _ => Err(ResolvError("Resolving invalid type".into())),
        }
    }

    pub fn resolve(&self, expr: Rc<Expr>) -> Result<Rc<Expr>> {
        if match expr.as_ref() {
            Expr::Object(im_map) => false,
            Expr::Int(_) => false,
            Expr::String(_) => false,
            Expr::Var(_) => true,
            Expr::FuncDefIdent(_, expr) => false,
            Expr::FuncDefPattern(items, expr) => false,
            Expr::Let(items, expr) => true,
            Expr::FuncCall(_, expr) => true,
            Expr::BoundExpr(scope, expr) => true,
        } {
            self.resolve(self.resolve_once(expr)?)
        } else {
            Ok(expr)
        }
    }

    pub fn get_item(&self, expr: Rc<Expr>, item: &str) -> Result<Rc<Expr>> {
        match expr.as_ref() {
            Expr::Object(fields) => {
                let field = fields
                    .get(item)
                    .ok_or_else(|| Error::ResolvError("field not found".into()))?;
                Ok(field.clone())
            }
            Expr::Let(_, _) => self.get_item(self.resolve_once(expr)?, item),
            Expr::BoundExpr(_, _) => self.get_item(self.resolve_once(expr)?, item),
            _ => Err(Error::ResolvError("get_item resolving".into())),
        }
    }

    pub fn get_path<'a>(
        &self,
        expr: Rc<Expr>,
        path: impl Iterator<Item = &'a str>,
    ) -> Result<Rc<Expr>> {
        let mut cur = expr;
        for item in path {
            cur = self.get_item(cur, item)?;
        }
        Ok(cur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::DnjParser;

    #[test]
    fn test_eval() {
        let expr = DnjParser::parse_str(
            r#"
                {
                    stuff = "hello";
                    something = "hej";
                }
            "#,
        )
        .unwrap();
        let scope = Scope::default();
        let value = scope.get_item(expr, "stuff").unwrap();
        assert_eq!(*value, Expr::String("hello".into()));
    }

    #[test]
    fn test_eval_deep() {
        // This also tests "inner" as prefixed for reserved keyword "in" is ok
        let expr = DnjParser::parse_str(
            r#"
                {
                    stuff = "hello";
                    something = {
                        inner = "deep";
                    };
                }
            "#,
        )
        .unwrap();
        let scope = Scope::default();
        let value = scope
            .get_path(expr, vec!["something", "inner"].into_iter())
            .unwrap();
        assert_eq!(*value, Expr::String("deep".into()));
    }

    #[test]
    fn test_let() {
        let expr = DnjParser::parse_str(
            r#"
                let
                    a = 12;
                    b = "hello";
                in
                b
            "#,
        )
        .unwrap();
        let scope = Scope::default();
        let value = scope.resolve(expr).unwrap();
        assert_eq!(*value, Expr::String("hello".into()));
    }

    #[test]
    fn test_invalid_var() {
        let expr = DnjParser::parse_str(
            r#"
                invalid_var
            "#,
        )
        .unwrap();
        let scope = Scope::default();
        let value = scope.resolve(expr).unwrap_err();
        assert_eq!(value, Error::ResolvError("Unknown variable".into()));
    }

    #[test]
    fn test_x() {
        let expr = DnjParser::parse_str(
            r#"
                let
                    a = 12;
                in
                {
                    stuff = a;
                }
            "#,
        )
        .unwrap();
        let scope = Scope::default();
        let value = scope.resolve(scope.get_item(expr, "stuff").unwrap()).unwrap();
        assert_eq!(*value, Expr::Int(12));
    }
}
