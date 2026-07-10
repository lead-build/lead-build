use super::error::{Error, ErrorType, Result};
use super::expr::{
    Exportable, Expr, ExprBinOp, ExprMapType, ExprOps, ExprSet, ExprType, ExprUnOp,
    matcher::{Matcher, ObjectMatch},
};
use super::stringdecode::{StringType, string_decode};
use std::fmt::{Debug, Display};

// Types just to reduce the complexity within the grammar file. Those are only
// there to silence out clippy for now...

type TypeSwitchCase<T, F> = (Expr<T, F>, Expr<T, F>);
type TypeLetSetStmt<T, F> = (Matcher<T, F>, Expr<T, F>);
type TypeAssignStmt<T, F> = (String, Expr<T, F>);

lalrpop_mod!(grammar, "lang/grammar.rs");

use lalrpop_util::lalrpop_mod;

type IntParseError<'input> = lalrpop_util::ParseError<usize, grammar::Token<'input>, &'static str>;

pub trait ParsableValue
where
    Self: Sized,
{
    fn parse_int(value: impl ToString) -> Option<Self>;
    fn parse_string(value: impl ToString) -> Option<Self>;
    fn from_bool(value: bool) -> Self;
}

fn transform_parse_error<F>(input: IntParseError, file: &F) -> Error<F>
where
    F: Clone,
{
    match input {
        lalrpop_util::ParseError::InvalidToken { location } => {
            Error::new(ErrorType::Parse, "Invalid token").loc(location, location, file)
        }
        lalrpop_util::ParseError::UnrecognizedEof { location, expected } => Error::new(
            ErrorType::Parse,
            format!("Unexpected end of file, expected {}", expected.join(", ")),
        )
        .loc(location, location, file),
        lalrpop_util::ParseError::UnrecognizedToken {
            token: (left, token, right),
            expected,
        } => Error::new(
            ErrorType::Parse,
            format!(
                "Unrecognized token: {}, expected {}",
                token,
                expected.join(", ")
            ),
        )
        .loc(left, right, file),

        lalrpop_util::ParseError::ExtraToken {
            token: (left, token, right),
        } => Error::new(ErrorType::Parse, format!("Extra token: {}", token)).loc(left, right, file),
        lalrpop_util::ParseError::User { error } => {
            Error::new(ErrorType::Parse, error).loc(0, 0, file)
        }
    }
}

pub fn parse_str<T, F>(code: &str, file: &F) -> Result<Expr<T, F>, F>
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps<F> + Exportable + Debug,
    F: Clone + Debug,
{
    let parser = grammar::ExprParser::new();
    let result = parser
        .parse::<T, F>(file, code)
        .map_err(|e| transform_parse_error(e, file))?;
    Ok(result)
}

fn unescape_str(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars();

    let _ = chars.next(); // TODO: expect "

    while let Some(c) = match chars.next() {
        Some('"') => None,
        Some('\\') => match chars.next() {
            Some('n') => Some('\n'),
            Some('r') => Some('\r'),
            Some('t') => Some('\t'),
            Some('u') => {
                let hex: String = [
                    chars.next().unwrap(),
                    chars.next().unwrap(),
                    chars.next().unwrap(),
                    chars.next().unwrap(),
                ]
                .iter()
                .collect();
                let u: u32 = u32::from_str_radix(hex.as_str(), 16).unwrap();
                let c = char::from_u32(u).unwrap();
                Some(c)
            }
            Some(c) => Some(c),
            None => panic!("Unmatched escape seq"),
        },
        Some(c) => Some(c),
        None => panic!("invalid string"),
    } {
        out.push(c);
    }

    out
}

fn unpack_str<T, F>(input: String, left: usize, right: usize, file: &F) -> Result<Expr<T, F>, F>
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps<F> + Exportable + Debug,
    F: Clone + Debug,
{
    let parts_values = string_decode(input.as_str())
        .unwrap()
        .into_iter()
        .map(|part| match part {
            StringType::Str(s) => Ok(ExprType::Value(T::parse_string(s).ok_or_else(|| {
                Error::new(ErrorType::Parse, "Error parsing string").loc(left, right, file)
            })?)
            .toexpr(left, right, file)),
            StringType::Expr(code) => parse_str(&code, file),
        })
        .collect::<Result<Vec<Expr<T, F>>, F>>()?;

    if parts_values.len() == 1 {
        Ok(parts_values.into_iter().next().unwrap())
    } else {
        Ok(ExprType::Concat(parts_values).toexpr(left, right, file))
    }
}

fn unpack_int<T, F>(input: &str) -> ExprType<T, F>
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    match T::parse_int(input) {
        Some(value) => value.into(),
        None => panic!("Error parsing int"),
    }
}

fn unpack_bool<T, F>(input: bool) -> ExprType<T, F>
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    T::from_bool(input).into()
}

#[cfg(test)]
mod tests {
    use super::super::testvalue::TestValue;
    use super::*;

    type FRef = i32;

    fn eval<'a>(code: &str) -> Expr<TestValue, FRef> {
        parse_str(code, &1).unwrap()
    }

    #[test]
    fn test_parse_int() {
        assert_eq!(
            ExprType::from(ExprType::Value(TestValue::Int(1231))).builtin(),
            eval("1231")
        );
    }

    #[test]
    fn test_parse_obj() {
        let code = r#"
            {
                boll = 123;
                hej = 323;
            }
        "#;
        assert_eq!(
            ExprType::from(ExprType::Object(ExprSet::from([
                (
                    "boll".into(),
                    ExprType::Value(TestValue::Int(123)).builtin()
                ),
                ("hej".into(), ExprType::Value(TestValue::Int(323)).builtin())
            ])))
            .builtin(),
            eval(code)
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
        assert_eq!(
            ExprType::from(ExprType::Object(ExprSet::from([
                (
                    "boll".into(),
                    ExprType::Value(TestValue::Int(123)).builtin()
                ),
                (
                    "hej".into(),
                    ExprType::Object(ExprSet::from([
                        ("a".into(), ExprType::Value(TestValue::Int(2)).builtin()),
                        ("b".into(), ExprType::Value(TestValue::Int(3)).builtin()),
                    ]))
                    .builtin()
                )
            ])))
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_str_unicode() {
        let code = "\"boll\\\"hej\\u0041\"";
        assert_eq!(
            ExprType::from(ExprType::Value(TestValue::String("boll\"hejA".into()))).builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_func_call() {
        let code = "hej 12";
        assert_eq!(
            ExprType::from(ExprType::FuncCall(
                ExprType::Value(TestValue::Int(12)).builtin(),
                ExprType::Var("hej".into()).builtin(),
            ))
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_func_def_pattern_non_var_1() {
        let code = "{ hej, hopp, svej }: 12";

        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str(code, &1);
        // Should be an error, try to unwrap it. Panic otherwise
        let _ = res.unwrap_err();
    }

    #[test]
    fn test_parse_func_def_pattern_non_var_2() {
        let code = "{ hej, hopp, svej, }: 12";

        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str(code, &1);
        // Should be an error, try to unwrap it. Panic otherwise
        let _ = res.unwrap_err();
    }

    #[test]
    fn test_parse_let() {
        let code = "let a = 21; b = 33; in 434";
        assert_eq!(
            ExprType::from(ExprType::Let(
                vec![
                    (
                        Matcher::Ident("a".into()),
                        ExprType::Value(TestValue::Int(21)).builtin()
                    ),
                    (
                        Matcher::Ident("b".into()),
                        ExprType::Value(TestValue::Int(33)).builtin()
                    ),
                ],
                ExprType::Value(TestValue::Int(434)).builtin(),
            ))
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_add_mul_prio() {
        let code = "2 * 3 + 4 * 5";
        assert_eq!(
            ExprType::from(ExprType::BinOp(
                ExprBinOp::Add,
                ExprType::BinOp(
                    ExprBinOp::Mult,
                    ExprType::Value(TestValue::Int(2)).builtin(),
                    ExprType::Value(TestValue::Int(3)).builtin()
                )
                .builtin(),
                ExprType::BinOp(
                    ExprBinOp::Mult,
                    ExprType::Value(TestValue::Int(4)).builtin(),
                    ExprType::Value(TestValue::Int(5)).builtin()
                )
                .builtin()
            ))
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_bool_op() {
        let code = "false || true";
        assert_eq!(
            ExprType::from(ExprType::BinOp(
                ExprBinOp::LogOr,
                ExprType::Value(TestValue::Bool(false)).builtin(),
                ExprType::Value(TestValue::Bool(true)).builtin(),
            ))
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_list() {
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[]", &1);
        res.unwrap();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[1]", &1);
        res.unwrap();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[1,2]", &1);
        res.unwrap();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[1,2,]", &1);
        res.unwrap();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[,1,2]", &1);
        res.unwrap_err();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[1,,2]", &1);
        res.unwrap_err();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[1,2,,]", &1);
        res.unwrap_err();
    }

    #[test]
    fn test_parse_comment_line_only() {
        let code = "# this is a comment\n123";
        assert_eq!(
            ExprType::from(ExprType::Value(TestValue::Int(123))).builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_comment_trailing() {
        let code = "let a = 21; b = 33; # this is ignored\nin 434";
        assert_eq!(
            ExprType::from(ExprType::Let(
                vec![
                    (
                        Matcher::Ident("a".into()),
                        ExprType::Value(TestValue::Int(21)).builtin()
                    ),
                    (
                        Matcher::Ident("b".into()),
                        ExprType::Value(TestValue::Int(33)).builtin()
                    ),
                ],
                ExprType::Value(TestValue::Int(434)).builtin(),
            ))
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_comment_eof() {
        let code = "123 # trailing comment with no newline";
        assert_eq!(
            ExprType::from(ExprType::Value(TestValue::Int(123))).builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_hash_in_string() {
        let code = "\"abc#def\"";
        assert_eq!(
            ExprType::from(ExprType::Value(TestValue::String("abc#def".into()))).builtin(),
            eval(code)
        );
    }
}
