use std::collections::BTreeMap;

#[derive(Debug)]
pub enum Error {
    ResolvError(String),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, PartialEq)]
pub enum Expr {
    Object(Box<BTreeMap<String, Expr>>),
    Int(i64),
    String(String),
    FuncDefIdent(String, Box<Expr>),
    FuncDefPattern(Vec<String>, Box<Expr>),

    Let(Vec<(String, Box<Expr>)>, Box<Expr>),
    FuncCall(String, Box<Expr>),
}

#[derive(Debug)]
pub struct Scope {
    vars: BTreeMap<String, Expr>,
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            vars: BTreeMap::new(),
        }
    }
}

impl Expr {
    fn resolve(&self) -> Result<&Expr> {
        match self {
            Expr::Let(_items, expr) => Ok(expr),
            Expr::FuncCall(_name, _expr) => todo!(),
            primitive => Ok(primitive)
        }
    }

    pub fn get_item(&self, item: &str) -> Result<&Expr> {
        let resolved = self.resolve()?;
        match resolved {
            Expr::Object(fields) => fields
                .get(item)
                .ok_or_else(|| Error::ResolvError("field not found".into())),
            _ => Err(Error::ResolvError("get_item resolving".into())),
        }
    }

    pub fn get_path<'a>(&self, path: impl Iterator<Item = &'a str>) -> Result<&Expr> {
        let mut cur = self;
        for item in path {
            let cur_resolved = cur.resolve()?;
            cur = cur_resolved.get_item(item)?;
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
        let expr: Expr = DnjParser::parse_str(
            r#"
                {
                    stuff = "hello";
                    something = "hej";
                }
            "#,
        )
        .unwrap();
        let value: &Expr = expr.get_item("stuff").unwrap();
        assert_eq!(value, &Expr::String("hello".into()));
    }

    #[test]
    fn test_eval_deep() {
        let expr: Expr = DnjParser::parse_str(
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
        let value: &Expr = expr
            .get_path(vec!["something", "inner"].into_iter())
            .unwrap();
        assert_eq!(value, &Expr::String("deep".into()));
    }

    #[test]
    fn test_let_noref() {
        let expr: Expr = DnjParser::parse_str(
            r#"
                let
                    a = 12;
                    b = 13;
                in
                {
                    stuff = "hello";
                    something = "hej";
                }
            "#,
        )
        .unwrap();
        let value: &Expr = expr.get_item("stuff").unwrap();
        assert_eq!(value, &Expr::String("hello".into()));
    }
}
