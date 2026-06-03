use crate::error::{DnjError, Result};
use crate::expr::{Expr, ExprBinOp, ExprSet, ExprType, ExprUnOp};
use crate::value::Value;
use std::fs;
use std::path::PathBuf;
lalrpop_mod!(grammar);

use lalrpop_util::lalrpop_mod;

type Ex = Expr<Value>;
type ExT = ExprType<Value>;
type ExSet = ExprSet<Value>;

pub trait ParsableValue
where
    Self: Sized,
{
    fn parse_int(value: impl ToString) -> Option<Self>;
    fn parse_string(value: impl ToString) -> Option<Self>;
}

pub fn parse_file(path: PathBuf) -> Result<Expr<Value>> {
    let parser: grammar::ExprParser = grammar::ExprParser::new();
    let code = fs::read_to_string(path).unwrap();
    match parser.parse(&code) {
        Ok(res) => Ok(res),
        Err(err) => Err(DnjError::ParseError(err.to_string())),
    }
}

pub fn parse_str(code: &str) -> Result<Expr<Value>> {
    let parser = grammar::ExprParser::new();
    match parser.parse(code) {
        Ok(res) => Ok(res),
        Err(err) => Err(DnjError::ParseError(err.to_string())),
    }
}

fn unpack_str(input: &str) -> Ex {
    let mut out = String::new();
    let mut chars = input.chars();

    let _ = chars.next(); // TODO: expect "

    while let Some(c) = match chars.next() {
        Some('"') => None,
        Some('\\') => match chars.next() {
            Some('n') => Some('\n'),
            Some('r') => Some('\r'),
            Some('t') => Some('\t'),
            Some(c) => Some(c),
            None => panic!("Unmatched escape seq"),
        },
        Some(c) => Some(c),
        None => panic!("invalid string"),
    } {
        out.push(c);
    }
    Ex::from(Value::parse_string(out).unwrap())
}

fn unpack_int(input: &str) -> Ex {
    Ex::from(Value::parse_int(input).unwrap())
}

fn unpack_bool(input: bool) -> Ex {
    Ex::from(Value::Bool(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_int() {
        let tree: Expr<Value> = parse_str("1231").unwrap();
        assert_eq!(Expr::from(ExprType::Value(Value::Int(1231))), tree);
    }

    #[test]
    fn test_parse_obj() {
        let code = r#"
            {
                boll = 123;
                hej = 323;
            }
        "#;
        let tree = parse_str(code).unwrap();
        assert_eq!(
            Expr::from(ExprType::Object(
                ExprSet::from([
                    ("boll", ExprType::Value(Value::Int(123)).into()),
                    ("hej", ExprType::Value(Value::Int(323)).into())
                ])
                .unwrap()
            )),
            tree
        );
    }

    #[test]
    fn test_parse_obj_in_obj() {
        let code = r#"
            {
                boll = 123;
                hej = { a=2; b=3; };
            }
        "#;
        let tree = parse_str(code).unwrap();
        assert_eq!(
            Expr::from(ExprType::Object(
                ExprSet::from([
                    ("boll", ExprType::Value(Value::Int(123)).into()),
                    (
                        "hej".into(),
                        ExprType::Object(
                            ExprSet::from([
                                ("a", ExprType::Value(Value::Int(2)).into()),
                                ("b", ExprType::Value(Value::Int(3)).into()),
                            ])
                            .unwrap()
                        )
                        .into()
                    )
                ])
                .unwrap()
            )),
            tree
        );
    }

    #[test]
    fn test_parse_str() {
        let code = "\"boll\\\"hej\\u0041\"";
        let tree = parse_str(code).unwrap();
        assert_eq!(
            Expr::from(ExprType::Value(Value::String("boll\"hejA".into()))),
            tree
        );
    }

    #[test]
    fn test_parse_func_call() {
        let code = "hej 12";
        let tree = parse_str(code).unwrap();
        assert_eq!(
            Expr::from(ExprType::FuncCall(
                "hej".into(),
                ExprType::Value(Value::Int(12)).into()
            )),
            tree
        );
    }

    #[test]
    fn test_parse_func_def_ident() {
        let code = "hej: 12";
        let tree = parse_str(code).unwrap();
        assert_eq!(
            Expr::from(ExprType::FuncDefIdent(
                "hej".into(),
                ExprType::Value(Value::Int(12)).into()
            )),
            tree
        );
    }

    #[test]
    fn test_parse_func_def_pattern_variadic() {
        let code = "{ hej, hopp, svej, ... }: 12";
        let tree = parse_str(code).unwrap();
        assert_eq!(
            Expr::from(ExprType::FuncDefPattern(
                vec!["hej".into(), "hopp".into(), "svej".into()],
                ExprType::Value(Value::Int(12)).into()
            )),
            tree
        );
    }

    #[test]
    fn test_parse_func_def_pattern_non_var_1() {
        let code = "{ hej, hopp, svej }: 12";

        let res: Result<Expr<Value>> = parse_str(code);
        // Should be an error, try to unwrap it. Panic otherwise
        let _ = res.unwrap_err();
    }

    #[test]
    fn test_parse_func_def_pattern_non_var_2() {
        let code = "{ hej, hopp, svej, }: 12";

        let res: Result<Expr<Value>> = parse_str(code);
        // Should be an error, try to unwrap it. Panic otherwise
        let _ = res.unwrap_err();
    }

    #[test]
    fn test_parse_let() {
        let code = "let a = 21; b = 33; in 434";
        let tree = parse_str(code).unwrap();
        assert_eq!(
            Expr::from(ExprType::Let(
                vec![
                    ("a".into(), ExprType::Value(Value::Int(21)).into()),
                    ("b".into(), ExprType::Value(Value::Int(33)).into()),
                ],
                ExprType::Value(Value::Int(434)).into(),
            )),
            tree
        );
    }

    #[test]
    fn test_parse_add_mul_prio() {
        let code = "2 * 3 + 4 * 5";
        let tree = parse_str(code).unwrap();
        assert_eq!(
            Expr::from(ExprType::BinOp(
                ExprBinOp::Add,
                ExprType::BinOp(
                    ExprBinOp::Mult,
                    ExprType::Value(Value::Int(2)).into(),
                    ExprType::Value(Value::Int(3)).into()
                )
                .into(),
                ExprType::BinOp(
                    ExprBinOp::Mult,
                    ExprType::Value(Value::Int(4)).into(),
                    ExprType::Value(Value::Int(5)).into()
                )
                .into()
            )),
            tree
        );
    }

    #[test]
    fn test_bool_op() {
        let code = "false || true";
        let tree = parse_str(code).unwrap();
        assert_eq!(
            Expr::from(ExprType::BinOp(
                ExprBinOp::LogOr,
                ExprType::Value(Value::Bool(false)).into(),
                ExprType::Value(Value::Bool(true)).into(),
            )),
            tree
        );
    }
}
