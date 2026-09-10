use super::error::{Error, ErrorType, Loc, Result};
use super::expr::{
    Exportable, Expr, ExprBinOp, ExprMapType, ExprOps, ExprSet, ExprType, ExprUnOp, Matcher,
};
use crate::pbexpr::Referrable;
use crate::pblang::{
    self,
    syntaxtree::{SyntaxKind, SyntaxNode, SyntaxToken},
    visit::{AssignKey, AttrSelector, LangVisitor, MapKind, ObjectField, StringPart, walk_expr},
};
use crate::strkey::StrKey;
use rowan::NodeOrToken;
use rowan::TextRange;
use std::fmt::{Debug, Display};

pub trait ParsableValue
where
    Self: Sized,
{
    fn parse_int(value: impl ToString) -> Option<Self>;
    fn parse_string(value: impl ToString) -> Option<Self>;
    fn from_bool(value: bool) -> Self;
}

/// Splits a `ParseError` into its byte span, a short category label, and a
/// detail message — kept apart (rather than one combined string) so each
/// can land on its own line in the rendered error block.
fn parse_error_parts(err: &pblang::ParseError) -> (std::ops::Range<usize>, &'static str, String) {
    match err {
        lalrpop_util::ParseError::InvalidToken { location } => {
            (*location..*location, "Invalid token", String::new())
        }
        lalrpop_util::ParseError::UnrecognizedEof { location, expected } => (
            *location..*location,
            "Unexpected end of file",
            format!("expected one of: {}", expected.join(", ")),
        ),
        lalrpop_util::ParseError::UnrecognizedToken {
            token: (start, token, end),
            expected,
        } => (
            *start..*end,
            "Unrecognized token",
            format!("found `{token}`, expected one of: {}", expected.join(", ")),
        ),
        lalrpop_util::ParseError::ExtraToken {
            token: (start, token, end),
        } => (*start..*end, "Extra token", format!("found `{token}`")),
        lalrpop_util::ParseError::User { error } => {
            (error.span.clone(), "Lexer error", error.message.clone())
        }
    }
}

/// Renders one `ParseError` as a three-line block: location, category,
/// detail message (the last omitted when there's nothing beyond the
/// category itself, e.g. `InvalidToken`).
fn parse_error_block<F>(err: pblang::ParseError, file: &F) -> String
where
    F: Clone + Referrable,
{
    let (span, kind, detail) = parse_error_parts(&err);
    let loc = Loc {
        file: file.clone(),
        span,
    };
    if detail.is_empty() {
        format!("{loc}\n{kind}")
    } else {
        format!("{loc}\n{kind}\n{detail}")
    }
}

/// Combines every recovered parse error (and, if parsing ultimately failed
/// outright, the final fatal one too) into a single `Error` with one
/// blank-line-separated block per error — rather than surfacing only the
/// first, as a plain `Result` would.
fn transform_parse_errors<F>(
    recovered: Vec<pblang::ErrorRecovery>,
    fatal: Option<pblang::ParseError>,
    file: &F,
) -> Error<F>
where
    F: Clone + Referrable,
{
    let mut blocks: Vec<String> = recovered
        .into_iter()
        .map(|recovery| parse_error_block(recovery.error, file))
        .collect();
    if let Some(fatal) = fatal {
        blocks.push(parse_error_block(fatal, file));
    }
    Error::new(ErrorType::Parse, blocks.join("\n\n"))
}

pub fn parse_str<T, F>(code: &str, file: &F) -> Result<Expr<T, F>, F>
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps<F> + Exportable + Debug,
    F: Clone + Debug + Referrable,
{
    let parsed = pblang::parse(code);
    match parsed.tree {
        Ok(tree) if parsed.errors.is_empty() => {
            let mut generator = ExprGenerator {
                file,
                _value: std::marker::PhantomData,
            };
            walk_expr(&tree, &mut generator)
        }
        Ok(_) => Err(transform_parse_errors(parsed.errors, None, file)),
        Err(fatal) => Err(transform_parse_errors(parsed.errors, Some(fatal), file)),
    }
}

/// Decodes escape sequences in a raw string chunk (as produced by the lexer:
/// no surrounding quotes, `${`/`"` already split out as separate tokens).
fn unescape_chunk(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars();

    while let Some(c) = match chars.next() {
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
        Some('$') => match chars.next() {
            Some('$') => Some('$'),
            _ => unreachable!("lexer only allows '$' as part of a '$$' escape"),
        },
        Some(c) => Some(c),
        None => None,
    } {
        out.push(c);
    }

    out
}

fn range_usize(range: rowan::TextRange) -> (usize, usize) {
    (usize::from(range.start()), usize::from(range.end()))
}

fn binary_op_from_kind(kind: SyntaxKind) -> ExprBinOp {
    match kind {
        SyntaxKind::QUESTION => ExprBinOp::HasAttr,
        SyntaxKind::PLUS_PLUS => ExprBinOp::ListConcat,
        SyntaxKind::STAR => ExprBinOp::Mult,
        SyntaxKind::SLASH => ExprBinOp::Div,
        SyntaxKind::MINUS => ExprBinOp::Sub,
        SyntaxKind::PLUS => ExprBinOp::Add,
        SyntaxKind::SLASH_SLASH => ExprBinOp::Update,
        SyntaxKind::LT => ExprBinOp::Lt,
        SyntaxKind::LT_EQ => ExprBinOp::Le,
        SyntaxKind::GT => ExprBinOp::Gt,
        SyntaxKind::GT_EQ => ExprBinOp::Ge,
        SyntaxKind::EQ_EQ => ExprBinOp::Eq,
        SyntaxKind::BANG_EQ => ExprBinOp::Neq,
        SyntaxKind::AMP_AMP => ExprBinOp::LogAnd,
        SyntaxKind::PIPE_PIPE => ExprBinOp::LogOr,
        SyntaxKind::THIN_ARROW => ExprBinOp::LogImpl,
        other => unreachable!("BINARY_EXPR has unexpected operator token {other:?}"),
    }
}

/// Reconstructs an object key's raw (still-escaped) text from its
/// `STRING_LIT` node. Object keys don't support string interpolation: they
/// must be resolvable to a `StrKey` without evaluating any expression, since
/// keys are fixed once when the tree is built, not lazily evaluated. This is
/// its own scan (rather than going through `walk_expr`/`visit_string`)
/// because a key is resolved eagerly to a `StrKey`, not lowered into a lazy
/// `Expr`.
fn string_key_text<F>(node: &SyntaxNode, file: &F) -> Result<String, F>
where
    F: Clone,
{
    let mut raw = String::new();
    for el in node.children_with_tokens() {
        match el {
            NodeOrToken::Token(t) if t.kind() == SyntaxKind::STRING_CHUNK => {
                raw.push_str(t.text());
            }
            NodeOrToken::Node(n) if n.kind() == SyntaxKind::STRING_EMBED => {
                let (start, end) = range_usize(node.text_range());
                return Err(Error::new(
                    ErrorType::Parse,
                    "object keys cannot contain string interpolation",
                )
                .loc(start, end, file));
            }
            _ => {}
        }
    }
    Ok(raw)
}

struct ExprGenerator<'a, T, F> {
    file: &'a F,
    _value: std::marker::PhantomData<T>,
}

impl<T, F> ExprGenerator<'_, T, F>
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps<F> + Exportable + Debug,
    F: Clone + Debug + Referrable,
{
    fn expr(&self, kind: ExprType<T, F>, range: TextRange) -> Expr<T, F> {
        let (start, end) = range_usize(range);
        kind.toexpr(start, end, self.file)
    }

    /// Resolves an `AssignKey` (as extracted by `walk_expr`) to a `StrKey`,
    /// eagerly — unlike a normal expression, an assignment key must be known
    /// without evaluating anything, since it's used as a `BTreeMap` key.
    fn resolve_assign_key(&self, key: AssignKey) -> Result<StrKey, F> {
        match key {
            AssignKey::Ident(token) => Ok(StrKey::from(token.text())),
            AssignKey::StringLit(node) => {
                let raw = string_key_text(&node, self.file)?;
                Ok(StrKey::from(unescape_chunk(&raw).as_str()))
            }
        }
    }
}

impl<T, F> LangVisitor for ExprGenerator<'_, T, F>
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps<F> + Exportable + Debug,
    F: Clone + Debug + Referrable,
{
    type Expr = Expr<T, F>;
    type Matcher = Matcher<T, F>;
    type Error = Error<F>;

    fn visit_let(
        &mut self,
        range: TextRange,
        bindings: Vec<(TextRange, Matcher<T, F>, Expr<T, F>)>,
        body: Expr<T, F>,
    ) -> Result<Expr<T, F>, F> {
        let bindings = bindings
            .into_iter()
            .map(|(_, matcher, value)| (matcher, value))
            .collect();
        Ok(self.expr(ExprType::Let(bindings, body), range))
    }

    fn visit_bind(
        &mut self,
        range: TextRange,
        items: Vec<(TextRange, AssignKey, Expr<T, F>)>,
        body: Expr<T, F>,
    ) -> Result<Expr<T, F>, F> {
        let items = items
            .into_iter()
            .map(|(_, key, value)| Ok((self.resolve_assign_key(key)?, value)))
            .collect::<Result<ExprSet<T, F>, F>>()?;
        Ok(self.expr(ExprType::Bind(items, body), range))
    }

    fn visit_func_def(
        &mut self,
        range: TextRange,
        params: Vec<Matcher<T, F>>,
        body: Expr<T, F>,
    ) -> Result<Expr<T, F>, F> {
        let mut result = body;
        for matcher in params.into_iter().rev() {
            result = self.expr(ExprType::FuncDef(matcher, result), range);
        }
        Ok(result)
    }

    fn visit_binary(
        &mut self,
        range: TextRange,
        op: SyntaxToken,
        lhs: Expr<T, F>,
        rhs: Expr<T, F>,
    ) -> Result<Expr<T, F>, F> {
        let op = binary_op_from_kind(op.kind());
        Ok(self.expr(ExprType::BinOp(op, lhs, rhs), range))
    }

    fn visit_unary(
        &mut self,
        range: TextRange,
        op: SyntaxToken,
        operand: Expr<T, F>,
    ) -> Result<Expr<T, F>, F> {
        let op = match op.kind() {
            SyntaxKind::MINUS => ExprUnOp::Neg,
            SyntaxKind::BANG => ExprUnOp::Not,
            other => unreachable!("UNARY_EXPR has unexpected operator {other:?}"),
        };
        Ok(self.expr(ExprType::UnOp(op, operand), range))
    }

    fn visit_func_call(
        &mut self,
        range: TextRange,
        func: Expr<T, F>,
        arg: Expr<T, F>,
    ) -> Result<Expr<T, F>, F> {
        Ok(self.expr(ExprType::FuncCall { func, arg }, range))
    }

    fn visit_attr_sel(
        &mut self,
        range: TextRange,
        base: Expr<T, F>,
        attr: AttrSelector<Expr<T, F>>,
    ) -> Result<Expr<T, F>, F> {
        let attr = match attr {
            AttrSelector::Dynamic(expr) => expr,
            AttrSelector::Static(name) => self.expr(
                ExprType::Value(T::new_from_string(name.text())),
                name.text_range(),
            ),
        };
        Ok(self.expr(ExprType::AttrSel(base, attr), range))
    }

    fn visit_fold(
        &mut self,
        range: TextRange,
        func: Expr<T, F>,
        init: Option<Expr<T, F>>,
        input: Expr<T, F>,
    ) -> Result<Expr<T, F>, F> {
        Ok(self.expr(ExprType::Fold { func, init, input }, range))
    }

    fn visit_map(
        &mut self,
        range: TextRange,
        kind: MapKind,
        func: Expr<T, F>,
        input: Expr<T, F>,
        filter: Option<Expr<T, F>>,
    ) -> Result<Expr<T, F>, F> {
        let kind = match kind {
            MapKind::List => ExprMapType::List,
            MapKind::Object => ExprMapType::Object,
        };
        Ok(self.expr(ExprType::Map(kind, func, input, filter), range))
    }

    fn visit_switch(
        &mut self,
        range: TextRange,
        input: Expr<T, F>,
        cases: Vec<(Expr<T, F>, Expr<T, F>)>,
        default: Option<Expr<T, F>>,
    ) -> Result<Expr<T, F>, F> {
        Ok(self.expr(ExprType::Switch(input, cases, default), range))
    }

    fn visit_object(
        &mut self,
        range: TextRange,
        items: Vec<(TextRange, AssignKey, Expr<T, F>)>,
    ) -> Result<Expr<T, F>, F> {
        let items = items
            .into_iter()
            .map(|(_, key, value)| Ok((self.resolve_assign_key(key)?, value)))
            .collect::<Result<ExprSet<T, F>, F>>()?;
        Ok(self.expr(ExprType::Object(items), range))
    }

    fn visit_list(&mut self, range: TextRange, items: Vec<Expr<T, F>>) -> Result<Expr<T, F>, F> {
        Ok(self.expr(ExprType::List(items), range))
    }

    fn visit_tuple(&mut self, range: TextRange, items: Vec<Expr<T, F>>) -> Result<Expr<T, F>, F> {
        Ok(self.expr(ExprType::Tuple(items), range))
    }

    fn visit_literal(&mut self, range: TextRange, token: SyntaxToken) -> Result<Expr<T, F>, F> {
        Ok(match token.kind() {
            SyntaxKind::TRUE_KW => self.expr(ExprType::Value(T::from_bool(true)), range),
            SyntaxKind::FALSE_KW => self.expr(ExprType::Value(T::from_bool(false)), range),
            SyntaxKind::NUMBER => self.expr(
                ExprType::Value(T::parse_int(token.text()).expect("Error parsing int")),
                range,
            ),
            SyntaxKind::NULL_KW => self.expr(ExprType::Null, range),
            other => unreachable!("LITERAL_EXPR wraps unexpected token {other:?}"),
        })
    }

    fn visit_string(
        &mut self,
        range: TextRange,
        parts: Vec<StringPart<Expr<T, F>>>,
    ) -> Result<Expr<T, F>, F> {
        let (start, end) = range_usize(range);
        let pieces = parts
            .into_iter()
            .map(|part| match part {
                StringPart::Chunk(token) => T::parse_string(unescape_chunk(token.text()))
                    .map(|value| self.expr(ExprType::Value(value), range))
                    .ok_or_else(|| {
                        Error::new(ErrorType::Parse, "Error parsing string")
                            .loc(start, end, self.file)
                    }),
                StringPart::Embed(expr) => Ok(expr),
            })
            .collect::<Result<Vec<_>, F>>()?;
        Ok(if pieces.len() == 1 {
            pieces.into_iter().next().unwrap()
        } else {
            self.expr(ExprType::Concat(pieces), range)
        })
    }

    fn visit_var(&mut self, range: TextRange, name: SyntaxToken) -> Result<Expr<T, F>, F> {
        Ok(self.expr(ExprType::Var(StrKey::from(name.text())), range))
    }

    fn visit_matcher_ident(
        &mut self,
        _range: TextRange,
        name: SyntaxToken,
    ) -> Result<Matcher<T, F>, F> {
        Ok(Matcher::Ident(StrKey::from(name.text())))
    }

    fn visit_matcher_wildcard(&mut self, _range: TextRange) -> Result<Matcher<T, F>, F> {
        Ok(Matcher::DontCare)
    }

    fn visit_matcher_alias(
        &mut self,
        _range: TextRange,
        inner: Matcher<T, F>,
        name: SyntaxToken,
    ) -> Result<Matcher<T, F>, F> {
        Ok(Matcher::Alias(Box::new(inner), StrKey::from(name.text())))
    }

    fn visit_matcher_tuple(
        &mut self,
        _range: TextRange,
        items: Vec<Matcher<T, F>>,
    ) -> Result<Matcher<T, F>, F> {
        Ok(Matcher::Tuple(items))
    }

    fn visit_matcher_object(
        &mut self,
        _range: TextRange,
        exhaustive: bool,
        fields: Vec<ObjectField<Matcher<T, F>, Expr<T, F>>>,
    ) -> Result<Matcher<T, F>, F> {
        let fields = fields
            .into_iter()
            .map(|field| {
                let key = StrKey::from(field.key.text());
                let matcher = field.matcher.unwrap_or(Matcher::Ident(key));
                (key, matcher, field.default)
            })
            .collect();
        Ok(Matcher::Object(fields, exhaustive))
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
    fn test_parse_bind() {
        let code = "bind a = 21; b = 33; in 434";
        assert_eq!(
            ExprType::Bind(
                ExprSet::from([
                    (
                        StrKey::from("a"),
                        ExprType::Value(TestValue::Int(21)).builtin()
                    ),
                    (
                        StrKey::from("b"),
                        ExprType::Value(TestValue::Int(33)).builtin()
                    ),
                ]),
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
}
