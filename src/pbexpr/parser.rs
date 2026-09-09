use super::error::{Error, ErrorType, Result};
use super::expr::matcher::ObjectMatch;
use super::expr::{
    Exportable, Expr, ExprBinOp, ExprMapType, ExprOps, ExprSet, ExprType, ExprUnOp, Matcher,
};
use crate::pbexpr::Referrable;
use crate::pblang::{
    self,
    syntaxtree::{SyntaxKind, SyntaxNode, SyntaxToken},
};
use crate::strkey::StrKey;
use rowan::NodeOrToken;
use std::fmt::{Debug, Display};

pub trait ParsableValue
where
    Self: Sized,
{
    fn parse_int(value: impl ToString) -> Option<Self>;
    fn parse_string(value: impl ToString) -> Option<Self>;
    fn from_bool(value: bool) -> Self;
}

fn transform_parse_error<F>(input: pblang::ParseError, file: &F) -> Error<F>
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
            token: (start, token, end),
            expected,
        } => Error::new(
            ErrorType::Parse,
            format!(
                "Unrecognized token: {}, expected {}",
                token,
                expected.join(", ")
            ),
        )
        .loc(start, end, file),

        lalrpop_util::ParseError::ExtraToken {
            token: (start, token, end),
        } => Error::new(ErrorType::Parse, format!("Extra token: {}", token)).loc(start, end, file),
        lalrpop_util::ParseError::User { error } => {
            Error::new(ErrorType::Parse, error).loc(0, 0, file)
        }
    }
}

/// Renders one `ParseError` as `"<message> (<location>)"`, reusing
/// `transform_parse_error` for both parts (every variant it produces
/// always carries exactly one location).
fn one_line<F>(err: pblang::ParseError, file: &F) -> String
where
    F: Clone + Referrable,
{
    let sub = transform_parse_error(err, file);
    match sub.locs.first() {
        Some(loc) => format!("{} ({})", sub.msg, loc),
        None => sub.msg,
    }
}

/// Combines every recovered parse error (and, if parsing ultimately failed
/// outright, the final fatal one too) into a single numbered `Error` —
/// rather than surfacing only the first, as a plain `Result` would.
fn transform_parse_errors<F>(
    recovered: Vec<pblang::ErrorRecovery>,
    fatal: Option<pblang::ParseError>,
    file: &F,
) -> Error<F>
where
    F: Clone + Referrable,
{
    let mut lines: Vec<String> = recovered
        .into_iter()
        .map(|recovery| one_line(recovery.error, file))
        .collect();
    if let Some(fatal) = fatal {
        lines.push(one_line(fatal, file));
    }
    let msg = lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| format!("{}. {}", i + 1, line))
        .collect::<Vec<_>>()
        .join("\n");
    Error::new(ErrorType::Parse, msg)
}

pub fn parse_str<T, F>(code: &str, file: &F) -> Result<Expr<T, F>, F>
where
    T: ParsableValue + Clone + PartialEq + Display + ExprOps<F> + Exportable + Debug,
    F: Clone + Debug + Referrable,
{
    let parsed = pblang::parse(code);
    match parsed.tree {
        Ok(tree) if parsed.errors.is_empty() => ExprGenerator {
            file,
            _value: std::marker::PhantomData,
        }
        .visit_expr(&tree),
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

fn span_of(node: &SyntaxNode) -> (usize, usize) {
    range_usize(node.text_range())
}

/// Direct, non-trivia token children of `node`, in source order. `TRIVIA` is
/// the synthetic space-padding `green_node` inserts for skipped
/// whitespace/comments (see `pblang::syntaxtree`); it never carries meaning.
fn non_trivia_tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + '_ {
    node.children_with_tokens()
        .filter_map(|el| el.into_token())
        .filter(|t| t.kind() != SyntaxKind::TRIVIA)
}

fn first_non_trivia_token(node: &SyntaxNode) -> SyntaxToken {
    non_trivia_tokens(node)
        .next()
        .unwrap_or_else(|| panic!("{:?} has at least one real token", node.kind()))
}

fn find_token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    non_trivia_tokens(node).find(|t| t.kind() == kind)
}

fn has_token(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    find_token(node, kind).is_some()
}

/// The single non-trivia operator token of a `BINARY_EXPR`/`UNARY_EXPR`
/// (the one thing between/before its expression child/children).
fn operator_token(node: &SyntaxNode) -> SyntaxToken {
    non_trivia_tokens(node)
        .next()
        .unwrap_or_else(|| panic!("{:?} has an operator token", node.kind()))
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

/// One piece of a `STRING_LIT`: either a raw literal chunk, or a `${..}`
/// interpolation (the `STRING_EMBED` node wrapping it).
enum StringPart {
    Chunk(String),
    Embed(SyntaxNode),
}

fn string_lit_parts(node: &SyntaxNode) -> Vec<StringPart> {
    node.children_with_tokens()
        .filter_map(|el| match el {
            NodeOrToken::Token(t) if t.kind() == SyntaxKind::STRING_CHUNK => {
                Some(StringPart::Chunk(t.text().to_string()))
            }
            NodeOrToken::Node(n) if n.kind() == SyntaxKind::STRING_EMBED => {
                Some(StringPart::Embed(n))
            }
            _ => None,
        })
        .collect()
}

/// Reconstructs an object key's raw (still-escaped) text from its
/// `STRING_LIT` node. Object keys don't support string interpolation: they
/// must be resolvable to a `StrKey` without evaluating any expression, since
/// keys are fixed once when the tree is built, not lazily evaluated.
fn string_key_text<F>(node: &SyntaxNode, file: &F) -> Result<String, F>
where
    F: Clone,
{
    let mut raw = String::new();
    for part in string_lit_parts(node) {
        match part {
            StringPart::Chunk(chunk) => raw.push_str(&chunk),
            StringPart::Embed(_) => {
                let (start, end) = span_of(node);
                return Err(Error::new(
                    ErrorType::Parse,
                    "object keys cannot contain string interpolation",
                )
                .loc(start, end, file));
            }
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
    fn expr(&self, kind: ExprType<T, F>, start: usize, end: usize) -> Expr<T, F> {
        kind.toexpr(start, end, self.file)
    }

    /// Splits an `ASSIGNMENT` node into its key (as a `StrKey`) and its value
    /// node. The key is either a bare `IDENT` token, or a `STRING_LIT` node
    /// (always the first node child when present).
    fn assignment_key_value(&self, assignment: &SyntaxNode) -> Result<(StrKey, SyntaxNode), F> {
        if let Some(ident) = find_token(assignment, SyntaxKind::IDENT) {
            let value = assignment
                .children()
                .next()
                .expect("ASSIGNMENT always has a value");
            return Ok((StrKey::from(ident.text()), value));
        }
        let mut children = assignment.children();
        let key_node = children.next().expect("ASSIGNMENT key is a STRING_LIT");
        let value = children.next().expect("ASSIGNMENT always has a value");
        let raw = string_key_text(&key_node, self.file)?;
        Ok((StrKey::from(unescape_chunk(&raw).as_str()), value))
    }

    fn visit_object_matcher_field(&self, field: &SyntaxNode) -> Result<ObjectMatch<T, F>, F> {
        let key = StrKey::from(first_non_trivia_token(field).text());
        let has_eq = has_token(field, SyntaxKind::EQ);
        let has_default = has_token(field, SyntaxKind::QUESTION);
        let children: Vec<SyntaxNode> = field.children().collect();
        let (matcher, default) = match (has_eq, has_default) {
            (false, false) => (Matcher::Ident(key), None),
            (true, false) => (self.visit_matcher(&children[0])?, None),
            (false, true) => (Matcher::Ident(key), Some(self.visit_expr(&children[0])?)),
            (true, true) => (
                self.visit_matcher(&children[0])?,
                Some(self.visit_expr(&children[1])?),
            ),
        };
        Ok((key, matcher, default))
    }

    fn visit_expr(&self, node: &SyntaxNode) -> Result<Expr<T, F>, F> {
        let (start, end) = span_of(node);
        let children: Vec<SyntaxNode> = node.children().collect();

        let value = match node.kind() {
            SyntaxKind::GROUP_EXPR => return self.visit_expr(&children[0]),

            SyntaxKind::LET_EXPR => {
                let body = children.last().expect("LET_EXPR has a body");
                let bindings = &children[..children.len() - 1];
                self.expr(
                    ExprType::Let(
                        bindings
                            .iter()
                            .map(|binding| {
                                let bc: Vec<SyntaxNode> = binding.children().collect();
                                Ok((self.visit_matcher(&bc[0])?, self.visit_expr(&bc[1])?))
                            })
                            .collect::<Result<_, F>>()?,
                        self.visit_expr(body)?,
                    ),
                    start,
                    end,
                )
            }
            SyntaxKind::BIND_EXPR => {
                let body = children.last().expect("BIND_EXPR has a body");
                let bindings = &children[..children.len() - 1];
                let items = bindings
                    .iter()
                    .map(|assignment| {
                        let (key, value) = self.assignment_key_value(assignment)?;
                        Ok((key, self.visit_expr(&value)?))
                    })
                    .collect::<Result<ExprSet<T, F>, F>>()?;
                self.expr(ExprType::Bind(items, self.visit_expr(body)?), start, end)
            }
            SyntaxKind::FUNC_DEF => {
                let body = children.last().expect("FUNC_DEF has a body");
                let matchers = &children[..children.len() - 1];
                let mut result = self.visit_expr(body)?;
                for matcher in matchers.iter().rev() {
                    result = self.expr(
                        ExprType::FuncDef(self.visit_matcher(matcher)?, result),
                        start,
                        end,
                    );
                }
                result
            }
            SyntaxKind::BINARY_EXPR => {
                let op = binary_op_from_kind(operator_token(node).kind());
                self.expr(
                    ExprType::BinOp(
                        op,
                        self.visit_expr(&children[0])?,
                        self.visit_expr(&children[1])?,
                    ),
                    start,
                    end,
                )
            }
            SyntaxKind::UNARY_EXPR => {
                let op = match operator_token(node).kind() {
                    SyntaxKind::MINUS => ExprUnOp::Neg,
                    SyntaxKind::BANG => ExprUnOp::Not,
                    other => unreachable!("UNARY_EXPR has unexpected operator {other:?}"),
                };
                self.expr(
                    ExprType::UnOp(op, self.visit_expr(&children[0])?),
                    start,
                    end,
                )
            }
            SyntaxKind::FUNC_CALL => self.expr(
                ExprType::FuncCall {
                    func: self.visit_expr(&children[0])?,
                    arg: self.visit_expr(&children[1])?,
                },
                start,
                end,
            ),
            SyntaxKind::ATTR_SEL => {
                let lhs = self.visit_expr(&children[0])?;
                let attr = if children.len() == 2 {
                    let inner = children[1]
                        .children()
                        .next()
                        .expect("DYNAMIC_ATTR wraps an expr");
                    self.visit_expr(&inner)?
                } else {
                    let name = find_token(node, SyntaxKind::IDENT)
                        .expect("static ATTR_SEL has an IDENT token");
                    let (nstart, nend) = range_usize(name.text_range());
                    self.expr(
                        ExprType::Value(T::new_from_string(name.text())),
                        nstart,
                        nend,
                    )
                };
                self.expr(ExprType::AttrSel(lhs, attr), start, end)
            }
            SyntaxKind::FOLD_EXPR => {
                let has_init = has_token(node, SyntaxKind::COLON);
                let (func, init, input) = if has_init {
                    (&children[0], Some(&children[1]), &children[2])
                } else {
                    (&children[0], None, &children[1])
                };
                self.expr(
                    ExprType::Fold {
                        func: self.visit_expr(func)?,
                        init: init.map(|n| self.visit_expr(n)).transpose()?,
                        input: self.visit_expr(input)?,
                    },
                    start,
                    end,
                )
            }
            SyntaxKind::MAP_EXPR => {
                let kind = match first_non_trivia_token(node).kind() {
                    SyntaxKind::L_BRACKET => ExprMapType::List,
                    SyntaxKind::L_BRACE => ExprMapType::Object,
                    other => unreachable!("MAP_EXPR starts with unexpected token {other:?}"),
                };
                let has_filter = has_token(node, SyntaxKind::IF_KW);
                let (func, input, filter) = if has_filter {
                    (&children[0], &children[1], Some(&children[2]))
                } else {
                    (&children[0], &children[1], None)
                };
                self.expr(
                    ExprType::Map(
                        kind,
                        self.visit_expr(func)?,
                        self.visit_expr(input)?,
                        filter.map(|n| self.visit_expr(n)).transpose()?,
                    ),
                    start,
                    end,
                )
            }
            SyntaxKind::SWITCH_EXPR => {
                let mut iter = children.iter();
                let input = iter.next().expect("SWITCH_EXPR has an input");
                let mut cases = Vec::new();
                let mut default = None;
                for child in iter {
                    if child.kind() == SyntaxKind::SWITCH_CASE {
                        let cc: Vec<SyntaxNode> = child.children().collect();
                        cases.push((self.visit_expr(&cc[0])?, self.visit_expr(&cc[1])?));
                    } else {
                        default = Some(self.visit_expr(child)?);
                    }
                }
                self.expr(
                    ExprType::Switch(self.visit_expr(input)?, cases, default),
                    start,
                    end,
                )
            }
            SyntaxKind::OBJECT_EXPR => {
                let items = children
                    .iter()
                    .map(|assignment| {
                        let (key, value) = self.assignment_key_value(assignment)?;
                        Ok((key, self.visit_expr(&value)?))
                    })
                    .collect::<Result<ExprSet<T, F>, F>>()?;
                self.expr(ExprType::Object(items), start, end)
            }
            SyntaxKind::LIST_EXPR => self.expr(
                ExprType::List(
                    children
                        .iter()
                        .map(|item| self.visit_expr(item))
                        .collect::<Result<_, F>>()?,
                ),
                start,
                end,
            ),
            SyntaxKind::TUPLE_EXPR => self.expr(
                ExprType::Tuple(
                    children
                        .iter()
                        .map(|item| self.visit_expr(item))
                        .collect::<Result<_, F>>()?,
                ),
                start,
                end,
            ),
            SyntaxKind::LITERAL_EXPR => {
                let tok = first_non_trivia_token(node);
                match tok.kind() {
                    SyntaxKind::TRUE_KW => {
                        self.expr(ExprType::Value(T::from_bool(true)), start, end)
                    }
                    SyntaxKind::FALSE_KW => {
                        self.expr(ExprType::Value(T::from_bool(false)), start, end)
                    }
                    SyntaxKind::NUMBER => self.expr(
                        ExprType::Value(T::parse_int(tok.text()).expect("Error parsing int")),
                        start,
                        end,
                    ),
                    SyntaxKind::NULL_KW => self.expr(ExprType::Null, start, end),
                    other => unreachable!("LITERAL_EXPR wraps unexpected token {other:?}"),
                }
            }
            SyntaxKind::STRING_LIT => {
                let pieces = string_lit_parts(node)
                    .into_iter()
                    .map(|part| match part {
                        StringPart::Chunk(raw) => T::parse_string(unescape_chunk(&raw))
                            .map(|value| self.expr(ExprType::Value(value), start, end))
                            .ok_or_else(|| {
                                Error::new(ErrorType::Parse, "Error parsing string")
                                    .loc(start, end, self.file)
                            }),
                        StringPart::Embed(embed) => {
                            let inner =
                                embed.children().next().expect("STRING_EMBED wraps an expr");
                            self.visit_expr(&inner)
                        }
                    })
                    .collect::<Result<Vec<_>, F>>()?;
                if pieces.len() == 1 {
                    pieces.into_iter().next().unwrap()
                } else {
                    self.expr(ExprType::Concat(pieces), start, end)
                }
            }
            SyntaxKind::VAR_EXPR => {
                let name = first_non_trivia_token(node);
                self.expr(ExprType::Var(StrKey::from(name.text())), start, end)
            }
            other => unreachable!("unexpected expression node kind {other:?}"),
        };
        Ok(value)
    }

    fn visit_matcher(&self, node: &SyntaxNode) -> Result<Matcher<T, F>, F> {
        let children: Vec<SyntaxNode> = node.children().collect();
        Ok(match node.kind() {
            SyntaxKind::MATCHER_IDENT => {
                Matcher::Ident(StrKey::from(first_non_trivia_token(node).text()))
            }
            SyntaxKind::MATCHER_WILDCARD => Matcher::DontCare,
            SyntaxKind::MATCHER_ALIAS => {
                let inner = self.visit_matcher(&children[0])?;
                let name = find_token(node, SyntaxKind::IDENT).expect("MATCHER_ALIAS has a name");
                Matcher::Alias(Box::new(inner), StrKey::from(name.text()))
            }
            SyntaxKind::MATCHER_TUPLE => Matcher::Tuple(
                children
                    .iter()
                    .map(|item| self.visit_matcher(item))
                    .collect::<Result<_, F>>()?,
            ),
            SyntaxKind::MATCHER_OBJECT => {
                let exhaustive = !has_token(node, SyntaxKind::DOT_DOT_DOT);
                let fields = children
                    .iter()
                    .map(|field| self.visit_object_matcher_field(field))
                    .collect::<Result<Vec<ObjectMatch<T, F>>, F>>()?;
                Matcher::Object(fields, exhaustive)
            }
            other => unreachable!("unexpected matcher node kind {other:?}"),
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
