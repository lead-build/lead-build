use std::{rc::Rc};
use crate::immap::ImMap;

#[derive(Debug)]
pub enum Error {
    ResolvError(String),
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

#[derive(Debug)]
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
        Self {
            vars: ImMap::new(),
        }
    }
}

impl Scope {
    pub fn resolve(&self, expr: Rc<Expr>) -> Result<Rc<Expr>> {
        match expr.as_ref() {
            Expr::Let(fields, expr) => {
                todo!()
            }
            _ => Ok(expr),
        }
    }

    pub fn get_item(&self, expr: Rc<Expr>, item: &str) -> Result<Rc<Expr>> {
        let resolved = self.resolve(expr)?;
        match resolved.as_ref() {
            Expr::Object(fields) => {
                let field = fields
                    .get(item)
                    .ok_or_else(|| Error::ResolvError("field not found".into()))?;
                Ok(field.clone())
            }
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
        ).unwrap();
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
    #[should_panic(expected = "not yet implemented")]
    fn test_let() {
        let expr = DnjParser::parse_str(
            r#"
                let
                    a = 12;
                    b = "hello";
                in
                {
                    refered_a = a;
                    stuff = b;
                }
            "#,
        )
        .unwrap();
        let scope = Scope::default();
        let value = scope.get_item(expr, "stuff").unwrap();
        assert_eq!(*value, Expr::String("hello".into()));
    }
}
