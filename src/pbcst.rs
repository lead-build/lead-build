use crate::strkey::StrKey;
use lalrpop_util::lalrpop_mod;
use std::fmt::{Debug, Display};

lalrpop_mod!(pub grammar, "pbcst/grammar.rs");

pub type ParseError<'input> = lalrpop_util::ParseError<usize, grammar::Token<'input>, &'static str>;

pub trait Referrable {
    fn format_ref(
        &self,
        left: usize,
        right: usize,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result;
}

pub fn parse<'input, F>(
    code: &'input str,
    file: &F,
) -> std::result::Result<PbNode<F>, ParseError<'input>>
where
    F: Clone + Debug + Referrable,
{
    let mut tree = grammar::ExprParser::new().parse(file, code)?;
    tree.source = Some(code.into());
    Ok(tree)
}

#[derive(Debug, PartialEq, Clone)]
pub struct Loc<F> {
    pub file: F,
    pub left: usize,
    pub right: usize,
}

impl<F> Display for Loc<F>
where
    F: Referrable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.file.format_ref(self.left, self.right, f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PbNode<F> {
    pub kind: PbNodeKind<F>,
    pub loc: Loc<F>,
    pub source: Option<String>,
}

impl<F> PbNode<F> {
    pub fn new(kind: PbNodeKind<F>, left: usize, right: usize, file: &F) -> Self
    where
        F: Clone,
    {
        Self {
            kind,
            loc: Loc {
                file: file.clone(),
                left,
                right,
            },
            source: None,
        }
    }

    pub fn to_source(&self) -> String {
        if let Some(source) = &self.source {
            return source.clone();
        }
        match &self.kind {
            PbNodeKind::Group(value) => format!("({})", value.to_source()),
            PbNodeKind::Let(bindings, body) => format!(
                "let {}in {}",
                bindings
                    .iter()
                    .map(LetBinding::to_source)
                    .collect::<Vec<_>>()
                    .join(""),
                body.to_source()
            ),
            PbNodeKind::FuncDef(matchers, body) => format!(
                "|{}|{}",
                matchers
                    .iter()
                    .map(Matcher::to_source)
                    .collect::<Vec<_>>()
                    .join(" "),
                body.to_source()
            ),
            PbNodeKind::Binary(op, lhs, rhs) => {
                format!("{}{}{}", lhs.to_source(), op.to_source(), rhs.to_source())
            }
            PbNodeKind::Unary(op, rhs) => format!("{}{}", op.to_source(), rhs.to_source()),
            PbNodeKind::FuncCall(func, arg) => format!("{} {}", func.to_source(), arg.to_source()),
            PbNodeKind::AttrSel(lhs, Attr::Ident(name, _)) => {
                format!("{}.{}", lhs.to_source(), name)
            }
            PbNodeKind::AttrSel(lhs, Attr::Dynamic(rhs)) => {
                format!("{}.{{{}}}", lhs.to_source(), rhs.to_source())
            }
            PbNodeKind::Fold { func, init, input } => format!(
                "({} for {}{})",
                func.to_source(),
                init.as_ref()
                    .map(|value| format!("{}:", value.to_source()))
                    .unwrap_or_default(),
                input.to_source()
            ),
            PbNodeKind::Map {
                kind,
                func,
                input,
                filter,
            } => format!(
                "{}{} for {}{}{}",
                match kind {
                    MapKind::List => "[",
                    MapKind::Object => "{",
                },
                func.to_source(),
                input.to_source(),
                filter
                    .as_ref()
                    .map(|value| format!(" if {}", value.to_source()))
                    .unwrap_or_default(),
                match kind {
                    MapKind::List => "]",
                    MapKind::Object => "}",
                }
            ),
            PbNodeKind::Switch {
                input,
                cases,
                default,
            } => format!(
                "switch {} {{{}{}}}",
                input.to_source(),
                cases
                    .iter()
                    .map(SwitchCase::to_source)
                    .collect::<Vec<_>>()
                    .join(""),
                default
                    .as_ref()
                    .map(|value| format!("_=>{};", value.to_source()))
                    .unwrap_or_default()
            ),
            PbNodeKind::Object(items) => format!(
                "{{{}}}",
                items
                    .iter()
                    .map(Assignment::to_source)
                    .collect::<Vec<_>>()
                    .join("")
            ),
            PbNodeKind::List(items) => format!(
                "[{}{}]",
                items
                    .items
                    .iter()
                    .map(PbNode::to_source)
                    .collect::<Vec<_>>()
                    .join(","),
                if items.trailing { "," } else { "" }
            ),
            PbNodeKind::Tuple(items) => format!(
                "({}{})",
                items
                    .items
                    .iter()
                    .map(PbNode::to_source)
                    .collect::<Vec<_>>()
                    .join(","),
                if items.trailing { "," } else { "" }
            ),
            PbNodeKind::Bool(value) => value.to_string(),
            PbNodeKind::Int(value) | PbNodeKind::String(value) => value.clone(),
            PbNodeKind::Var(name) => name.to_string(),
            PbNodeKind::Null => "null".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PbNodeKind<F> {
    Group(Box<PbNode<F>>),
    Let(Vec<LetBinding<F>>, Box<PbNode<F>>),
    FuncDef(Vec<Matcher<F>>, Box<PbNode<F>>),
    Binary(BinaryOp, Box<PbNode<F>>, Box<PbNode<F>>),
    Unary(UnaryOp, Box<PbNode<F>>),
    FuncCall(Box<PbNode<F>>, Box<PbNode<F>>),
    AttrSel(Box<PbNode<F>>, Attr<F>),
    Fold {
        func: Box<PbNode<F>>,
        init: Option<Box<PbNode<F>>>,
        input: Box<PbNode<F>>,
    },
    Map {
        kind: MapKind,
        func: Box<PbNode<F>>,
        input: Box<PbNode<F>>,
        filter: Option<Box<PbNode<F>>>,
    },
    Switch {
        input: Box<PbNode<F>>,
        cases: Vec<SwitchCase<F>>,
        default: Option<Box<PbNode<F>>>,
    },
    Object(Vec<Assignment<F>>),
    List(Delimited<PbNode<F>>),
    Tuple(Delimited<PbNode<F>>),
    Bool(bool),
    Int(String),
    String(String),
    Var(StrKey),
    Null,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum BinaryOp {
    HasAttr,
    ListConcat,
    Mult,
    Div,
    Sub,
    Add,
    Update,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Neq,
    LogAnd,
    LogOr,
    LogImpl,
}
impl BinaryOp {
    fn to_source(self) -> &'static str {
        match self {
            Self::HasAttr => "?",
            Self::ListConcat => "++",
            Self::Mult => "*",
            Self::Div => "/",
            Self::Sub => "-",
            Self::Add => "+",
            Self::Update => "//",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Eq => "==",
            Self::Neq => "!=",
            Self::LogAnd => "&&",
            Self::LogOr => "||",
            Self::LogImpl => "->",
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}
impl UnaryOp {
    fn to_source(self) -> &'static str {
        match self {
            Self::Neg => "-",
            Self::Not => "!",
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum MapKind {
    List,
    Object,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Delimited<T> {
    pub items: Vec<T>,
    pub trailing: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Attr<F> {
    Ident(StrKey, Loc<F>),
    Dynamic(Box<PbNode<F>>),
}
#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding<F> {
    pub matcher: Matcher<F>,
    pub value: PbNode<F>,
    pub loc: Loc<F>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment<F> {
    pub key: Key<F>,
    pub value: PbNode<F>,
    pub loc: Loc<F>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Key<F> {
    Ident(StrKey, Loc<F>),
    String(String, Loc<F>),
}
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase<F> {
    pub matcher: PbNode<F>,
    pub value: PbNode<F>,
    pub loc: Loc<F>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Matcher<F> {
    pub kind: MatcherKind<F>,
    pub loc: Loc<F>,
}
impl<F> Matcher<F> {
    pub fn new(kind: MatcherKind<F>, left: usize, right: usize, file: &F) -> Self
    where
        F: Clone,
    {
        Self {
            kind,
            loc: Loc {
                file: file.clone(),
                left,
                right,
            },
        }
    }
    pub fn to_source(&self) -> String {
        match &self.kind {
            MatcherKind::Alias(matcher, name, _) => format!("{}@{}", matcher.to_source(), name),
            MatcherKind::DontCare => "_".into(),
            MatcherKind::Ident(name) => name.to_string(),
            MatcherKind::Tuple(items) => format!(
                "({}{})",
                items
                    .items
                    .iter()
                    .map(Matcher::to_source)
                    .collect::<Vec<_>>()
                    .join(","),
                if items.trailing { "," } else { "" }
            ),
            MatcherKind::Object { fields, exhaustive } => format!(
                "{{{}{}}}",
                fields
                    .iter()
                    .map(ObjectMatcher::to_source)
                    .collect::<Vec<_>>()
                    .join(","),
                if *exhaustive {
                    ""
                } else if fields.is_empty() {
                    "..."
                } else {
                    ",..."
                }
            ),
        }
    }
}
impl<F> LetBinding<F> {
    fn to_source(&self) -> String {
        format!("{}={};", self.matcher.to_source(), self.value.to_source())
    }
}
impl<F> Assignment<F> {
    fn to_source(&self) -> String {
        format!(
            "{}={};",
            match &self.key {
                Key::Ident(key, _) => key.to_string(),
                Key::String(key, _) => key.clone(),
            },
            self.value.to_source()
        )
    }
}
impl<F> SwitchCase<F> {
    fn to_source(&self) -> String {
        format!("{}=>{};", self.matcher.to_source(), self.value.to_source())
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum MatcherKind<F> {
    Alias(Box<Matcher<F>>, StrKey, Loc<F>),
    DontCare,
    Ident(StrKey),
    Tuple(Delimited<Matcher<F>>),
    Object {
        fields: Vec<ObjectMatcher<F>>,
        exhaustive: bool,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectMatcher<F> {
    pub key: StrKey,
    pub matcher: Matcher<F>,
    pub default: Option<PbNode<F>>,
    pub loc: Loc<F>,
}
impl<F> ObjectMatcher<F> {
    fn to_source(&self) -> String {
        format!(
            "{}{}{}",
            self.key,
            match &self.matcher.kind {
                MatcherKind::Ident(name) if *name == self.key => "".into(),
                _ => format!("={}", self.matcher.to_source()),
            },
            self.default
                .as_ref()
                .map(|value| format!("?{}", value.to_source()))
                .unwrap_or_default()
        )
    }
}

pub trait Visitor<T, F> {
    type ExprOutput;
    type MatcherOutput;
    fn visit_expr(&self, expr: &PbNode<F>) -> Self::ExprOutput;
    fn visit_matcher(&self, matcher: &Matcher<F>) -> Self::MatcherOutput;
}
