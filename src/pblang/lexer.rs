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

    #[regex(r"[a-zA-Z][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    #[regex(r"[0-9]+", |lex| lex.slice().to_string())]
    Number(String),

    #[token("\"")]
    StringQuote,

    // The following three are never produced by this lexer's own automatic
    // scanning: they're only reachable while lexing in string mode (see
    // `StrTok` and `Lexer::next` below), and are constructed manually there.
    #[display_override("string content")]
    StringChunk(String),
    #[display_override("${")]
    StringEmbedStart,
    #[display_override("}")]
    StringEmbedEnd,
}

/// Token set used while lexing the body of a string literal, i.e. after a
/// `Tok::StringQuote` has been seen and before its matching close. Kept
/// separate from `Tok` (no shared `skip` rules: whitespace inside a string is
/// significant) and switched to/from via `logos::Lexer::morph`.
#[derive(Logos, Clone, Debug, PartialEq)]
enum StrTok {
    /// A maximal run of literal string content, raw/undecoded (escape
    /// sequences and `$$` are kept as-is) so it can be written back out
    /// byte-for-byte by the exporter. Escape decoding happens later, once
    /// this chunk is handed to the expression parser.
    #[regex(r#"(\\.|\$\$|[^"$\\])+"#, |lex| lex.slice().to_string())]
    Chunk(String),

    #[token("${")]
    EmbedStart,

    #[token("\"")]
    Quote,
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

enum Mode<'input> {
    Code(logos::Lexer<'input, Tok>),
    Str(logos::Lexer<'input, StrTok>),
}

pub struct Lexer<'input> {
    mode: Option<Mode<'input>>,
    /// One entry per currently-open `${`, counting unmatched `{` seen since
    /// that embed started, so we can tell an embed's own closing `}` apart
    /// from a `}` that closes an ordinary brace construct (object literal,
    /// block, matcher, ...) nested inside the embed.
    embed_depth: Vec<i32>,
}

impl<'input> Lexer<'input> {
    pub fn new(src: &'input str) -> Self {
        Self {
            mode: Some(Mode::Code(Tok::lexer(src))),
            embed_depth: Vec::new(),
        }
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Result<(usize, Tok, usize), LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.mode.take().expect("lexer mode always restored") {
            Mode::Code(mut lex) => {
                let tok = lex.next();
                let span = lex.span();
                match tok {
                    None => {
                        self.mode = Some(Mode::Code(lex));
                        None
                    }
                    Some(Err(())) => {
                        let message = format!("unrecognized token {:?}", lex.slice());
                        self.mode = Some(Mode::Code(lex));
                        Some(Err(LexError { message, span }))
                    }
                    Some(Ok(Tok::StringQuote)) => {
                        self.mode = Some(Mode::Str(lex.morph()));
                        Some(Ok((span.start, Tok::StringQuote, span.end)))
                    }
                    Some(Ok(Tok::LBrace)) if !self.embed_depth.is_empty() => {
                        *self.embed_depth.last_mut().unwrap() += 1;
                        self.mode = Some(Mode::Code(lex));
                        Some(Ok((span.start, Tok::LBrace, span.end)))
                    }
                    Some(Ok(Tok::RBrace)) if !self.embed_depth.is_empty() => {
                        let depth = self.embed_depth.last_mut().unwrap();
                        if *depth > 0 {
                            *depth -= 1;
                            self.mode = Some(Mode::Code(lex));
                            Some(Ok((span.start, Tok::RBrace, span.end)))
                        } else {
                            self.embed_depth.pop();
                            self.mode = Some(Mode::Str(lex.morph()));
                            Some(Ok((span.start, Tok::StringEmbedEnd, span.end)))
                        }
                    }
                    Some(Ok(t)) => {
                        self.mode = Some(Mode::Code(lex));
                        Some(Ok((span.start, t, span.end)))
                    }
                }
            }
            Mode::Str(mut lex) => {
                let tok = lex.next();
                let span = lex.span();
                match tok {
                    None => {
                        self.mode = Some(Mode::Str(lex));
                        Some(Err(LexError {
                            message: "unterminated string literal".to_string(),
                            span,
                        }))
                    }
                    Some(Err(())) => {
                        let message = format!("unrecognized token {:?} in string", lex.slice());
                        self.mode = Some(Mode::Str(lex));
                        Some(Err(LexError { message, span }))
                    }
                    Some(Ok(StrTok::Chunk(s))) => {
                        self.mode = Some(Mode::Str(lex));
                        Some(Ok((span.start, Tok::StringChunk(s), span.end)))
                    }
                    Some(Ok(StrTok::EmbedStart)) => {
                        self.embed_depth.push(0);
                        self.mode = Some(Mode::Code(lex.morph()));
                        Some(Ok((span.start, Tok::StringEmbedStart, span.end)))
                    }
                    Some(Ok(StrTok::Quote)) => {
                        self.mode = Some(Mode::Code(lex.morph()));
                        Some(Ok((span.start, Tok::StringQuote, span.end)))
                    }
                }
            }
        }
    }
}
