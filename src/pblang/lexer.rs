use logos::Logos;

#[derive(Logos, Clone, Debug, PartialEq, logos_display::Display)]
#[logos(skip(r"[ \t\r\n]+"))]
#[logos(skip(r"#[^\n\r]*", allow_greedy = true))]
pub enum Tok {
    #[token("bind")]
    Bind,
    #[token("let")]
    Let,
    #[token("in")]
    In,
    #[token("if")]
    If,
    #[token("for")]
    For,
    #[token("switch")]
    Switch,
    #[token("null")]
    Null,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("@")]
    At,
    #[token("_")]
    Underscore,
    #[token("...")]
    TripleDots,
    #[token("=>")]
    RDoubleArrow,
    #[token(":")]
    Colon,
    #[token(";")]
    SemiColon,
    #[token("=")]
    Assign,
    #[token("|")]
    Pipe,
    #[token(".")]
    Dot,
    #[token(",")]
    Comma,
    #[token("-")]
    Minus,
    #[token("!")]
    UnOpNot,
    #[token("?")]
    BinOpHasAttr,
    #[token("++")]
    BinOpListConcat,
    #[token("*")]
    BinOpMult,
    #[token("/")]
    BinOpDiv,
    #[token("+")]
    BinOpAdd,
    #[token("//")]
    BinOpUpdate,
    #[token("<")]
    BinOpLt,
    #[token("<=")]
    BinOpLe,
    #[token(">")]
    BinOpGt,
    #[token(">=")]
    BinOpGe,
    #[token("==")]
    BinOpEq,
    #[token("!=")]
    BinOpNeq,
    #[token("&&")]
    BinOpLogAnd,
    #[token("||")]
    BinOpLogOr,
    #[token("->")]
    BinOpLogImpl,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("(")]
    LPar,
    #[token(")")]
    RPar,
    #[token("[")]
    LSqBracket,
    #[token("]")]
    RSqBracket,

    #[regex(
        r"[a-zA-Z][a-zA-Z0-9_]*",
        |lex| lex.slice().to_string()
    )]
    Ident(String),

    #[regex(
        r"[0-9]+",
        |lex| lex.slice().to_string()
    )]
    Number(String),

    #[regex(
        r#""([^"]|\\.)*""#,
        |lex| lex.slice().to_string()
    )]
    Str(String),
}
#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub span: std::ops::Range<usize>,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {:?}", self.message, self.span)
    }
}

impl std::error::Error for LexError {}

pub struct Lexer<'input> {
    inner: logos::Lexer<'input, Tok>,
}

impl<'input> Lexer<'input> {
    pub fn new(src: &'input str) -> Self {
        Self {
            inner: Tok::lexer(src),
        }
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Result<(usize, Tok, usize), LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        let tok = self.inner.next()?;
        let span = self.inner.span();
        match tok {
            Ok(t) => Some(Ok((span.start, t, span.end))),
            Err(()) => Some(Err(LexError {
                message: format!("unrecognized token {:?}", self.inner.slice()),
                span,
            })),
        }
    }
}
