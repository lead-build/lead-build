use crate::strkey::StrKey;
use lalrpop_util::lalrpop_mod;

pub mod export;

lalrpop_mod!(pub grammar, "pbcst/grammar.rs");

pub type ParseError<'input> = lalrpop_util::ParseError<usize, grammar::Token<'input>, &'static str>;

pub fn parse(code: &str) -> std::result::Result<PbNode, ParseError<'_>> {
    let mut tree = grammar::ExprParser::new().parse(&(), code)?;
    tree.source = Some(code.into());
    Ok(tree)
}

#[derive(Debug, PartialEq, Clone)]
pub struct Span {
    pub left: usize,
    pub right: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PbNode {
    pub kind: PbNodeKind,
    pub span: Option<Span>,
    pub source: Option<String>,
}

impl PbNode {
    pub fn new<F>(kind: PbNodeKind, left: usize, right: usize, _file: &F) -> Self {
        Self {
            kind,
            span: Some(Span { left, right }),
            source: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PbNodeKind {
    Group(Box<PbNode>),
    Let(Vec<LetBinding>, Box<PbNode>),
    Bind(Vec<Assignment>, Box<PbNode>),
    FuncDef(Vec<Matcher>, Box<PbNode>),
    Binary(BinaryOp, Box<PbNode>, Box<PbNode>),
    Unary(UnaryOp, Box<PbNode>),
    FuncCall(Box<PbNode>, Box<PbNode>),
    AttrSel(Box<PbNode>, Attr),
    Fold {
        func: Box<PbNode>,
        init: Option<Box<PbNode>>,
        input: Box<PbNode>,
    },
    Map {
        kind: MapKind,
        func: Box<PbNode>,
        input: Box<PbNode>,
        filter: Option<Box<PbNode>>,
    },
    Switch {
        input: Box<PbNode>,
        cases: Vec<SwitchCase>,
        default: Option<Box<PbNode>>,
    },
    Object(Vec<Assignment>),
    List(Delimited<PbNode>),
    Tuple(Delimited<PbNode>),
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
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
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
pub enum Attr {
    Ident(StrKey, Option<Span>),
    Dynamic(Box<PbNode>),
}
#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding {
    pub matcher: Matcher,
    pub value: PbNode,
    pub span: Option<Span>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub key: Key,
    pub value: PbNode,
    pub span: Option<Span>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Ident(StrKey, Option<Span>),
    String(String, Option<Span>),
}
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    pub matcher: PbNode,
    pub value: PbNode,
    pub span: Option<Span>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Matcher {
    pub kind: MatcherKind,
    pub span: Option<Span>,
}
impl Matcher {
    pub fn new<F>(kind: MatcherKind, left: usize, right: usize, _file: &F) -> Self {
        Self {
            kind,
            span: Some(Span { left, right }),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum MatcherKind {
    Alias(Box<Matcher>, StrKey, Option<Span>),
    DontCare,
    Ident(StrKey),
    Tuple(Delimited<Matcher>),
    Object {
        fields: Vec<ObjectMatcher>,
        exhaustive: bool,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectMatcher {
    pub key: StrKey,
    pub matcher: Matcher,
    pub default: Option<PbNode>,
    pub span: Option<Span>,
}

pub trait Visitor<T, F> {
    type ExprOutput;
    type MatcherOutput;
    fn visit_expr(&self, expr: &PbNode) -> Self::ExprOutput;
    fn visit_matcher(&self, matcher: &Matcher) -> Self::MatcherOutput;
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::pbcst::export::Exportable;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn fixture_files(root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for entry in fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files.extend(fixture_files(&path));
            } else if path.file_name().is_some_and(|name| name == "main.pbb") {
                files.push(path);
            }
        }
        files.sort();
        files
    }

    #[test]
    fn fixture_files_round_trip_through_cst() {
        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

        for path in fixture_files(&fixture_root) {
            let source = fs::read_to_string(&path).unwrap();
            let tree = parse(&source)
                .unwrap_or_else(|error| panic!("failed to parse {}: {error:?}", path.display()));
            let mut output = Vec::new();
            tree.export(&mut output).unwrap();

            assert_eq!(output, source.as_bytes(), "fixture {}", path.display());
        }
    }
}
