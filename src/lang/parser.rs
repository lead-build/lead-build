use super::expr::{Expr, ExprBinOp, ExprSet, ExprType, ExprUnOp, ops::ExprOps};
use std::fmt::Display;
use std::fs;
use std::path::PathBuf;
lalrpop_mod!(grammar, "lang/grammar.rs");

use lalrpop_util::{ParseError, lalrpop_mod};

#[derive(Debug)]
pub struct Error {
    msg: String,
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

type IntParseError<'input> = ParseError<usize, grammar::Token<'input>, &'static str>;
// type IntResult<'input, T> = std::result::Result<T, IntParseError<'input>>;

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Error {
            msg: value.to_string(),
        }
    }
}
impl From<String> for Error {
    fn from(value: String) -> Self {
        Error { msg: value }
    }
}
impl<'input> From<IntParseError<'input>> for Error {
    fn from(value: IntParseError<'input>) -> Self {
        value.to_string().into() // TODO: nicer error
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.msg.fmt(f)
    }
}

pub trait ParsableValue
where
    Self: Sized,
{
    fn parse_int(value: impl ToString) -> Option<Self>;
    fn parse_string(value: impl ToString) -> Option<Self>;
    fn from_bool(value: bool) -> Self;
}

#[derive(Default)]
pub struct Parser {
    parser: grammar::ExprParser,
}

impl Parser {
    pub fn new() -> Parser {
        Default::default()
    }

    pub fn parse_file<'a, T>(&'a self, path: PathBuf) -> Result<Expr<'a, T>>
    where
        T: ParsableValue + Clone + PartialEq + Display + ExprOps,
    {
        let code = fs::read_to_string(path).unwrap();
        let result = self.parser.parse::<T>(&code)?;
        Ok(result)
    }

    pub fn parse_str<'a, T>(&'a self, code: &str) -> Result<Expr<'a, T>>
    where
        T: ParsableValue + Clone + PartialEq + Display + ExprOps,
    {
        let result = self.parser.parse::<T>(code)?;
        Ok(result)
    }
}

fn unpack_str<'a, T>(input: &str) -> Expr<'a, T>
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps,
{
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
    match T::parse_string(out) {
        Some(value) => value.into(),
        None => panic!("Error parsing string"),
    }
}

fn unpack_int<'a, T>(input: &str) -> Expr<'a, T>
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps,
{
    match T::parse_int(input) {
        Some(value) => value.into(),
        None => panic!("Error parsing int"),
    }
}

fn unpack_bool<'a, T>(input: bool) -> Expr<'a, T>
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps,
{
    T::from_bool(input).into()
}

#[cfg(test)]
mod tests {
    use super::super::testvalue::TestValue;
    use super::*;

    fn eval<'a>(p: &'a Parser, code: &str) -> Expr<'a, TestValue> {
        p.parse_str(code).unwrap()
    }

    #[test]
    fn test_parse_int() {
        let p = Parser::new();
        assert_eq!(
            Expr::from(ExprType::Value(TestValue::Int(1231))),
            eval(&p, "1231")
        );
    }

    #[test]
    fn test_parse_obj() {
        let p = Parser::new();
        let code = r#"
            {
                boll = 123;
                hej = 323;
            }
        "#;
        assert_eq!(
            Expr::from(ExprType::Object(
                ExprSet::from([
                    ("boll", ExprType::Value(TestValue::Int(123)).into()),
                    ("hej", ExprType::Value(TestValue::Int(323)).into())
                ])
                .unwrap()
            )),
            eval(&p, code)
        );
    }

    #[test]
    fn test_parse_obj_in_obj() {
        let p = Parser::new();
        let code = r#"
            {
                boll = 123;
                hej = { a=2; b=3; };
            }
        "#;
        assert_eq!(
            Expr::from(ExprType::Object(
                ExprSet::from([
                    ("boll", ExprType::Value(TestValue::Int(123)).into()),
                    (
                        "hej".into(),
                        ExprType::Object(
                            ExprSet::from([
                                ("a", ExprType::Value(TestValue::Int(2)).into()),
                                ("b", ExprType::Value(TestValue::Int(3)).into()),
                            ])
                            .unwrap()
                        )
                        .into()
                    )
                ])
                .unwrap()
            )),
            eval(&p, code)
        );
    }

    #[test]
    fn test_parse_str() {
        let p = Parser::new();
        let code = "\"boll\\\"hej\\u0041\"";
        assert_eq!(
            Expr::from(ExprType::Value(TestValue::String("boll\"hejA".into()))),
            eval(&p, code)
        );
    }

    #[test]
    fn test_parse_func_call() {
        let p = Parser::new();
        let code = "hej 12";
        assert_eq!(
            Expr::from(ExprType::FuncCall(
                "hej".into(),
                ExprType::Value(TestValue::Int(12)).into()
            )),
            eval(&p, code)
        );
    }

    #[test]
    fn test_parse_func_def_ident() {
        let p = Parser::new();
        let code = "hej: 12";
        assert_eq!(
            Expr::from(ExprType::FuncDefIdent(
                "hej".into(),
                ExprType::Value(TestValue::Int(12)).into()
            )),
            eval(&p, code)
        );
    }

    #[test]
    fn test_parse_func_def_pattern_variadic() {
        let p = Parser::new();
        let code = "{ hej, hopp, svej, ... }: 12";
        assert_eq!(
            Expr::from(ExprType::FuncDefPattern(
                vec!["hej".into(), "hopp".into(), "svej".into()],
                ExprType::Value(TestValue::Int(12)).into()
            )),
            eval(&p, code)
        );
    }

    #[test]
    fn test_parse_func_def_pattern_non_var_1() {
        let p = Parser::new();
        let code = "{ hej, hopp, svej }: 12";

        let res: Result<Expr<TestValue>> = p.parse_str(code);
        // Should be an error, try to unwrap it. Panic otherwise
        let _ = res.unwrap_err();
    }

    #[test]
    fn test_parse_func_def_pattern_non_var_2() {
        let p = Parser::new();
        let code = "{ hej, hopp, svej, }: 12";

        let res: Result<Expr<TestValue>> = p.parse_str(code);
        // Should be an error, try to unwrap it. Panic otherwise
        let _ = res.unwrap_err();
    }

    #[test]
    fn test_parse_let() {
        let p = Parser::new();
        let code = "let a = 21; b = 33; in 434";
        assert_eq!(
            Expr::from(ExprType::Let(
                vec![
                    ("a".into(), ExprType::Value(TestValue::Int(21)).into()),
                    ("b".into(), ExprType::Value(TestValue::Int(33)).into()),
                ],
                ExprType::Value(TestValue::Int(434)).into(),
            )),
            eval(&p, code)
        );
    }

    #[test]
    fn test_parse_add_mul_prio() {
        let p = Parser::new();
        let code = "2 * 3 + 4 * 5";
        assert_eq!(
            Expr::from(ExprType::BinOp(
                ExprBinOp::Add,
                ExprType::BinOp(
                    ExprBinOp::Mult,
                    ExprType::Value(TestValue::Int(2)).into(),
                    ExprType::Value(TestValue::Int(3)).into()
                )
                .into(),
                ExprType::BinOp(
                    ExprBinOp::Mult,
                    ExprType::Value(TestValue::Int(4)).into(),
                    ExprType::Value(TestValue::Int(5)).into()
                )
                .into()
            )),
            eval(&p, code)
        );
    }

    #[test]
    fn test_bool_op() {
        let p = Parser::new();
        let code = "false || true";
        assert_eq!(
            Expr::from(ExprType::BinOp(
                ExprBinOp::LogOr,
                ExprType::Value(TestValue::Bool(false)).into(),
                ExprType::Value(TestValue::Bool(true)).into(),
            )),
            eval(&p, code)
        );
    }
}
