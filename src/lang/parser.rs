use super::error::{Error, ErrorType, Result};
use super::expr::matcher::ObjectMatch;
use super::expr::{
    Exportable, Expr, ExprBinOp, ExprMapType, ExprOps, ExprSet, ExprType, ExprUnOp, Matcher,
};
use super::stringdecode::{StringType, string_decode};
use crate::strkey::StrKey;
use crate::{
    lang::Referrable,
    pbcst::{self, Attr, BinaryOp, Key, MapKind, MatcherKind, PbNodeKind, Visitor},
};
use std::fmt::{Debug, Display};

pub trait ParsableValue
where
    Self: Sized,
{
    fn parse_int(value: impl ToString) -> Option<Self>;
    fn parse_string(value: impl ToString) -> Option<Self>;
    fn from_bool(value: bool) -> Self;
}

fn transform_parse_error<F>(input: pbcst::ParseError<'_>, file: &F) -> Error<F>
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
    F: Clone + Debug + Referrable,
{
    let tree = pbcst::parse(code, file).map_err(|error| transform_parse_error(error, file))?;
    pbcst::Visitor::<T, F>::visit_expr(&ExprGenerator, &tree)
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

struct ExprGenerator;

impl ExprGenerator {
    fn expr<T, F>(&self, kind: ExprType<T, F>, loc: &pbcst::Loc<F>) -> Expr<T, F>
    where
        T: ParsableValue + Clone + PartialEq + Display + ExprOps<F> + Exportable + Debug,
        F: Clone + Debug + Referrable,
    {
        kind.toexpr(loc.left, loc.right, &loc.file)
    }
}

impl<T, F> Visitor<T, F> for ExprGenerator
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps<F> + Exportable + Debug,
    F: Clone + Debug + Referrable,
{
    type ExprOutput = Result<Expr<T, F>, F>;
    type MatcherOutput = Result<Matcher<T, F>, F>;

    fn visit_expr(&self, expr: &pbcst::PbNode<F>) -> Self::ExprOutput {
        use BinaryOp::*;

        let value = match &expr.kind {
            PbNodeKind::Group(value) => self.visit_expr(value)?,
            PbNodeKind::Let(bindings, body) => self.expr(
                ExprType::Let(
                    bindings
                        .iter()
                        .map(|binding| {
                            Ok((
                                self.visit_matcher(&binding.matcher)?,
                                self.visit_expr(&binding.value)?,
                            ))
                        })
                        .collect::<Result<_, F>>()?,
                    self.visit_expr(body)?,
                ),
                &expr.loc,
            ),
            PbNodeKind::FuncDef(matchers, body) => {
                let mut result = self.visit_expr(body)?;
                for matcher in matchers.iter().rev() {
                    result = self.expr(
                        ExprType::FuncDef(self.visit_matcher(matcher)?, result),
                        &expr.loc,
                    );
                }
                result
            }
            PbNodeKind::Binary(op, lhs, rhs) => self.expr(
                ExprType::BinOp(
                    match op {
                        HasAttr => ExprBinOp::HasAttr,
                        ListConcat => ExprBinOp::ListConcat,
                        Mult => ExprBinOp::Mult,
                        Div => ExprBinOp::Div,
                        Sub => ExprBinOp::Sub,
                        Add => ExprBinOp::Add,
                        Update => ExprBinOp::Update,
                        Lt => ExprBinOp::Lt,
                        Le => ExprBinOp::Le,
                        Gt => ExprBinOp::Gt,
                        Ge => ExprBinOp::Ge,
                        Eq => ExprBinOp::Eq,
                        Neq => ExprBinOp::Neq,
                        LogAnd => ExprBinOp::LogAnd,
                        LogOr => ExprBinOp::LogOr,
                        LogImpl => ExprBinOp::LogImpl,
                    },
                    self.visit_expr(lhs)?,
                    self.visit_expr(rhs)?,
                ),
                &expr.loc,
            ),
            PbNodeKind::Unary(op, rhs) => self.expr(
                ExprType::UnOp(
                    match op {
                        pbcst::UnaryOp::Neg => ExprUnOp::Neg,
                        pbcst::UnaryOp::Not => ExprUnOp::Not,
                    },
                    self.visit_expr(rhs)?,
                ),
                &expr.loc,
            ),
            PbNodeKind::FuncCall(func, arg) => self.expr(
                ExprType::FuncCall {
                    func: self.visit_expr(func)?,
                    arg: self.visit_expr(arg)?,
                },
                &expr.loc,
            ),
            PbNodeKind::AttrSel(lhs, attr) => self.expr(
                ExprType::AttrSel(
                    self.visit_expr(lhs)?,
                    match attr {
                        Attr::Ident(name, loc) => {
                            self.expr(ExprType::Value(T::new_from_string(name)), loc)
                        }
                        Attr::Dynamic(value) => self.visit_expr(value)?,
                    },
                ),
                &expr.loc,
            ),
            PbNodeKind::Fold { func, init, input } => self.expr(
                ExprType::Fold {
                    func: self.visit_expr(func)?,
                    init: init
                        .as_ref()
                        .map(|value| self.visit_expr(value))
                        .transpose()?,
                    input: self.visit_expr(input)?,
                },
                &expr.loc,
            ),
            PbNodeKind::Map {
                kind,
                func,
                input,
                filter,
            } => self.expr(
                ExprType::Map(
                    match kind {
                        MapKind::List => ExprMapType::List,
                        MapKind::Object => ExprMapType::Object,
                    },
                    self.visit_expr(func)?,
                    self.visit_expr(input)?,
                    filter
                        .as_ref()
                        .map(|value| self.visit_expr(value))
                        .transpose()?,
                ),
                &expr.loc,
            ),
            PbNodeKind::Switch {
                input,
                cases,
                default,
            } => self.expr(
                ExprType::Switch(
                    self.visit_expr(input)?,
                    cases
                        .iter()
                        .map(|case| {
                            Ok((
                                self.visit_expr(&case.matcher)?,
                                self.visit_expr(&case.value)?,
                            ))
                        })
                        .collect::<Result<_, F>>()?,
                    default
                        .as_ref()
                        .map(|value| self.visit_expr(value))
                        .transpose()?,
                ),
                &expr.loc,
            ),
            PbNodeKind::Object(assignments) => self.expr(
                ExprType::Object(
                    assignments
                        .iter()
                        .map(|assignment| {
                            Ok((
                                match &assignment.key {
                                    Key::Ident(key, _) => *key,
                                    Key::String(raw, _) => StrKey::from(unescape_str(raw).as_str()),
                                },
                                self.visit_expr(&assignment.value)?,
                            ))
                        })
                        .collect::<Result<ExprSet<T, F>, F>>()?,
                ),
                &expr.loc,
            ),
            PbNodeKind::List(items) => self.expr(
                ExprType::List(
                    items
                        .items
                        .iter()
                        .map(|item| self.visit_expr(item))
                        .collect::<Result<_, F>>()?,
                ),
                &expr.loc,
            ),
            PbNodeKind::Tuple(items) => self.expr(
                ExprType::Tuple(
                    items
                        .items
                        .iter()
                        .map(|item| self.visit_expr(item))
                        .collect::<Result<_, F>>()?,
                ),
                &expr.loc,
            ),
            PbNodeKind::Bool(value) => self.expr(ExprType::Value(T::from_bool(*value)), &expr.loc),
            PbNodeKind::Int(value) => self.expr(
                ExprType::Value(T::parse_int(value).expect("Error parsing int")),
                &expr.loc,
            ),
            PbNodeKind::String(raw) => {
                let parts = string_decode(&unescape_str(raw))
                    .unwrap()
                    .into_iter()
                    .map(|part| match part {
                        StringType::Str(value) => T::parse_string(value)
                            .map(|value| self.expr(ExprType::Value(value), &expr.loc))
                            .ok_or_else(|| {
                                Error::new(ErrorType::Parse, "Error parsing string").loc(
                                    expr.loc.left,
                                    expr.loc.right,
                                    &expr.loc.file,
                                )
                            }),
                        StringType::Expr(code) => parse_str(&code, &expr.loc.file),
                    })
                    .collect::<Result<Vec<_>, F>>()?;
                if parts.len() == 1 {
                    parts.into_iter().next().unwrap()
                } else {
                    self.expr(ExprType::Concat(parts), &expr.loc)
                }
            }
            PbNodeKind::Var(name) => self.expr(ExprType::Var(*name), &expr.loc),
            PbNodeKind::Null => self.expr(ExprType::Null, &expr.loc),
        };
        Ok(value)
    }

    fn visit_matcher(&self, matcher: &pbcst::Matcher<F>) -> Self::MatcherOutput {
        Ok(match &matcher.kind {
            MatcherKind::Alias(inner, name, _) => {
                Matcher::Alias(Box::new(self.visit_matcher(inner)?), *name)
            }
            MatcherKind::DontCare => Matcher::DontCare,
            MatcherKind::Ident(name) => Matcher::Ident(*name),
            MatcherKind::Tuple(items) => Matcher::Tuple(
                items
                    .items
                    .iter()
                    .map(|item| self.visit_matcher(item))
                    .collect::<Result<_, F>>()?,
            ),
            MatcherKind::Object { fields, exhaustive } => Matcher::Object(
                fields
                    .iter()
                    .map(|field| {
                        Ok((
                            field.key,
                            self.visit_matcher(&field.matcher)?,
                            field
                                .default
                                .as_ref()
                                .map(|value| self.visit_expr(value))
                                .transpose()?,
                        ))
                    })
                    .collect::<Result<Vec<ObjectMatch<T, F>>, F>>()?,
                *exhaustive,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::expr::{ExprBinOp, ExprSet, ExprType, matcher::Matcher};
    use super::super::testvalue::{FRef, TestValue};
    use super::*;
    use crate::strkey::StrKey;

    fn eval(code: &str) -> Expr<TestValue, FRef> {
        parse_str(code, &FRef).unwrap()
    }

    #[test]
    fn test_parse_int() {
        assert_eq!(
            ExprType::Value(TestValue::Int(1231)).builtin(),
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
            ExprType::Object(ExprSet::from([
                (
                    StrKey::from("boll"),
                    ExprType::Value(TestValue::Int(123)).builtin()
                ),
                (
                    StrKey::from("hej"),
                    ExprType::Value(TestValue::Int(323)).builtin()
                )
            ]))
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
            ExprType::Object(ExprSet::from([
                (
                    StrKey::from("boll"),
                    ExprType::Value(TestValue::Int(123)).builtin()
                ),
                (
                    StrKey::from("hej"),
                    ExprType::Object(ExprSet::from([
                        (
                            StrKey::from("a"),
                            ExprType::Value(TestValue::Int(2)).builtin()
                        ),
                        (
                            StrKey::from("b"),
                            ExprType::Value(TestValue::Int(3)).builtin()
                        ),
                    ]))
                    .builtin()
                )
            ]))
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_str_unicode() {
        let code = "\"boll\\\"hej\\u0041\"";
        assert_eq!(
            ExprType::Value(TestValue::String("boll\"hejA".into())).builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_func_call() {
        let code = "hej 12";
        assert_eq!(
            ExprType::FuncCall {
                arg: ExprType::Value(TestValue::Int(12)).builtin(),
                func: ExprType::Var("hej".into()).builtin(),
            }
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_func_def_pattern_non_var_1() {
        let code = "{ hej, hopp, svej }: 12";

        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str(code, &FRef);
        // Should be an error, try to unwrap it. Panic otherwise
        let _ = res.unwrap_err();
    }

    #[test]
    fn test_parse_func_def_pattern_non_var_2() {
        let code = "{ hej, hopp, svej, }: 12";

        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str(code, &FRef);
        // Should be an error, try to unwrap it. Panic otherwise
        let _ = res.unwrap_err();
    }

    #[test]
    fn test_parse_let() {
        let code = "let a = 21; b = 33; in 434";
        assert_eq!(
            ExprType::Let(
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
            )
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_add_mul_prio() {
        let code = "2 * 3 + 4 * 5";
        assert_eq!(
            ExprType::BinOp(
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
            )
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_bool_op() {
        let code = "false || true";
        assert_eq!(
            ExprType::BinOp(
                ExprBinOp::LogOr,
                ExprType::Value(TestValue::Bool(false)).builtin(),
                ExprType::Value(TestValue::Bool(true)).builtin(),
            )
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_list() {
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[]", &FRef);
        res.unwrap();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[1]", &FRef);
        res.unwrap();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[1,2]", &FRef);
        res.unwrap();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[1,2,]", &FRef);
        res.unwrap();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[,1,2]", &FRef);
        res.unwrap_err();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[1,,2]", &FRef);
        res.unwrap_err();
        let res: Result<Expr<TestValue, FRef>, FRef> = parse_str("[1,2,,]", &FRef);
        res.unwrap_err();
    }

    #[test]
    fn test_parse_comment_line_only() {
        let code = "# this is a comment\n123";
        assert_eq!(ExprType::Value(TestValue::Int(123)).builtin(), eval(code));
    }

    #[test]
    fn test_parse_comment_trailing() {
        let code = "let a = 21; b = 33; # this is ignored\nin 434";
        assert_eq!(
            ExprType::Let(
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
            )
            .builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_comment_eof() {
        let code = "123 # trailing comment with no newline";
        assert_eq!(ExprType::Value(TestValue::Int(123)).builtin(), eval(code));
    }

    #[test]
    fn test_parse_hash_in_string() {
        let code = "\"abc#def\"";
        assert_eq!(
            ExprType::Value(TestValue::String("abc#def".into())).builtin(),
            eval(code)
        );
    }

    #[test]
    fn test_parse_cst_source_and_location() {
        let code = " let a = 1; in a + 2 ";
        let tree = pbcst::parse(code, &FRef).unwrap();
        assert_eq!(tree.to_source(), code);
        assert_eq!(tree.loc.left, 1);
        assert_eq!(tree.loc.right, code.len() - 1);

        assert_eq!(
            pbcst::parse("([1,2,])", &FRef).unwrap().to_source(),
            "([1,2,])"
        );
    }
}
