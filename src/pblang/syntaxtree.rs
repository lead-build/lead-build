//! Lossless-*shaped* syntax tree for pblang, built on [`rowan`].
//!
//! The grammar (`pblang::grammar`, generated from `grammar.lalrpop`) now
//! builds this tree directly via [`green_token`]/[`green_node`] instead of
//! the old `PbNode` CST, and [`super::parse`] hands callers the resulting
//! [`SyntaxNode`]. `pbexpr::parser` walks it by matching on [`SyntaxKind`]
//! instead of `PbNodeKind`.
//!
//! It isn't *actually* lossless yet, though: `pblang::lexer::Tok` still
//! skips whitespace/comments (`#[logos(skip(..))]`) rather than emitting
//! them as trivia tokens, so `green_node` has to reconstruct the gaps a
//! bottom-up grammar leaves between children as synthetic space padding (see
//! its doc comment) — same length as the original text, not the same bytes.
//! Making it byte-exact just needs the lexer to stop skipping and start
//! emitting real `TRIVIA` tokens instead; nothing else here would need to
//! change.

use rowan::{GreenNode, GreenToken, NodeOrToken};

/// The kinds of nodes and tokens that can appear in the tree. `rowan` stores
/// kinds as a plain `u16` (its own `rowan::SyntaxKind`); this enum is the
/// typed version we actually match on, converted at the boundary via the
/// `From` impl and the `Language` impl below.
///
/// The token half mirrors `pblang::lexer::Tok` one-for-one (plus `TRIVIA`,
/// for the whitespace/comments `Tok` currently throws away via
/// `#[logos(skip(..))]`). The node half mirrors `pblang::PbNodeKind` and the structs it
/// nests (`Assignment`, `LetBinding`, `SwitchCase`, `MatcherKind`,
/// `ObjectMatcher`, `Attr`) — each doc comment names the type it stands in
/// for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    /// Whitespace, comments, or any other source text that isn't part of the
    /// language proper. `Tok` doesn't tokenize this today (it's
    /// `#[logos(skip(..))]`), so it never reaches the tree from the lexer
    /// yet; for now it's also what [`green_node`] pads gaps with (as
    /// synthetic spaces) to keep a subtree's text length matching its
    /// `start`/`end`, until real trivia text flows through from the lexer.
    TRIVIA,

    // --- keywords (`Tok::Bind` etc.) ---
    BIND_KW,
    LET_KW,
    IN_KW,
    IF_KW,
    FOR_KW,
    SWITCH_KW,
    NULL_KW,
    TRUE_KW,
    FALSE_KW,

    // --- punctuation & operators (`Tok::At` etc.) ---
    AT,          // @
    UNDERSCORE,  // _
    DOT_DOT_DOT, // ...
    FAT_ARROW,   // =>
    COLON,       // :
    SEMICOLON,   // ;
    EQ,          // =
    PIPE,        // |
    DOT,         // .
    COMMA,       // ,
    MINUS,       // -
    BANG,        // !
    QUESTION,    // ?
    PLUS_PLUS,   // ++
    STAR,        // *
    SLASH,       // /
    PLUS,        // +
    SLASH_SLASH, // //
    LT,          // <
    LT_EQ,       // <=
    GT,          // >
    GT_EQ,       // >=
    EQ_EQ,       // ==
    BANG_EQ,     // !=
    AMP_AMP,     // &&
    PIPE_PIPE,   // ||
    THIN_ARROW,  // ->
    L_BRACE,     // {
    R_BRACE,     // }
    L_PAREN,     // (
    R_PAREN,     // )
    L_BRACKET,   // [
    R_BRACKET,   // ]

    // --- literals & string tokens (`Tok::Ident` etc.) ---
    IDENT,
    NUMBER,
    /// `Tok::StringQuote` — the opening or closing `"` of a string literal.
    STRING_QUOTE,
    /// `Tok::StringChunk` — a raw run of literal text inside a string.
    STRING_CHUNK,
    /// `Tok::StringEmbedStart` — the `${` opening an interpolated expression.
    STRING_EMBED_START,
    /// `Tok::StringEmbedEnd` — the `}` closing an interpolated expression.
    STRING_EMBED_END,

    ERROR,

    // --- nodes (composites, from `PbNodeKind` and friends) ---
    /// `PbNodeKind::Group` — `( <expr> )`.
    GROUP_EXPR,
    /// `PbNodeKind::Let` — `let <LET_BINDING>* in <expr>`.
    LET_EXPR,
    /// `LetBinding` — `<matcher> = <expr> ;`.
    LET_BINDING,
    /// `PbNodeKind::Bind` — `bind <ASSIGNMENT>* in <expr>`.
    BIND_EXPR,
    /// `Assignment` — `<key> = <expr> ;`, used by `BIND_EXPR` and `OBJECT_EXPR`.
    ASSIGNMENT,
    /// `PbNodeKind::FuncDef` — `|<matcher>*| <expr>`.
    FUNC_DEF,
    /// `PbNodeKind::Binary` — `<expr> <op> <expr>`; the operator is whichever
    /// punctuation token (`PLUS`, `EQ_EQ`, ...) sits between the children.
    BINARY_EXPR,
    /// `PbNodeKind::Unary` — `<op> <expr>` (`MINUS` or `BANG`).
    UNARY_EXPR,
    /// `PbNodeKind::FuncCall` — `<expr> <expr>`.
    FUNC_CALL,
    /// `PbNodeKind::AttrSel` — `<expr> . <Attr>`.
    ATTR_SEL,
    /// `Attr::Dynamic` — the `{ <expr> }` after a `.` in `ATTR_SEL`.
    DYNAMIC_ATTR,
    /// `PbNodeKind::Fold` — `(<expr> for [<expr> :] <expr>)`.
    FOLD_EXPR,
    /// `PbNodeKind::Map` — `[<expr> for <expr> [if <expr>]]` or the `{...}`
    /// form; `MapKind` is told apart by whether `L_BRACKET`/`R_BRACKET` or
    /// `L_BRACE`/`R_BRACE` bracket the node.
    MAP_EXPR,
    /// `PbNodeKind::Switch` — `switch <expr> { <SWITCH_CASE>* }`.
    SWITCH_EXPR,
    /// `SwitchCase` — `<matcher> => <expr> ;`.
    SWITCH_CASE,
    /// `PbNodeKind::Object` — `{ <ASSIGNMENT>* }`.
    OBJECT_EXPR,
    /// `PbNodeKind::List` — `[ <expr>,* ]`.
    LIST_EXPR,
    /// `PbNodeKind::Tuple` — `( <expr>,* )`.
    TUPLE_EXPR,
    /// `PbNodeKind::Bool` / `Int` / `Null` — wraps a single `TRUE_KW`,
    /// `FALSE_KW`, `NUMBER` or `NULL_KW` token.
    LITERAL_EXPR,
    /// `PbNodeKind::String` — `STRING_QUOTE (STRING_CHUNK | STRING_EMBED)*
    /// STRING_QUOTE`.
    STRING_LIT,
    /// `StringPart::Embed` — one `${ <expr> }` interpolation inside a
    /// `STRING_LIT`: `STRING_EMBED_START <expr> STRING_EMBED_END`.
    STRING_EMBED,
    /// `PbNodeKind::Var` — wraps a single `IDENT` token.
    VAR_EXPR,
    /// `MatcherKind::Ident` — wraps a single `IDENT` token.
    MATCHER_IDENT,
    /// `MatcherKind::DontCare` — wraps a single `UNDERSCORE` token.
    MATCHER_WILDCARD,
    /// `MatcherKind::Alias` — `<matcher> @ <ident>`.
    MATCHER_ALIAS,
    /// `MatcherKind::Tuple` — `( <matcher>,* )`.
    MATCHER_TUPLE,
    /// `MatcherKind::Object` — `{ <OBJECT_MATCHER_FIELD>,* [...] }`.
    MATCHER_OBJECT,
    /// `ObjectMatcher` — `<ident> [= <matcher>] [? <expr>]`.
    OBJECT_MATCHER_FIELD,
    /// `PbNodeKind::OutputPlaceholder` — synthetic, output-only: never
    /// produced by parsing source text.
    OUTPUT_PLACEHOLDER,
    /// `PbNodeKind::OutputElided` — synthetic, output-only.
    OUTPUT_ELIDED,
    /// `PbNodeKind::OutputCycle` — synthetic, output-only.
    OUTPUT_CYCLE,
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

/// Marker type tying [`SyntaxKind`] to rowan's generic `SyntaxNode`/`SyntaxToken`.
/// Uninhabited: it only ever exists as a type parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lang {}

impl rowan::Language for Lang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        assert!(raw.0 <= SyntaxKind::OUTPUT_CYCLE as u16);
        // SAFETY: `SyntaxKind` is `#[repr(u16)]` and the assert above checks
        // `raw.0` is in range, so this is a valid bit pattern for the enum.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<Lang>;
pub type SyntaxToken = rowan::SyntaxToken<Lang>;
pub type SyntaxElement = rowan::SyntaxElement<Lang>;

/// A node or token, plus the `(start, end)` byte range of source text it
/// covers. This is the uniform return type a bottom-up grammar (lalrpop)
/// hands back from every production, so a parent rule can combine its
/// children's `GreenSpan`s into its own via [`green_node`] without needing
/// any other bookkeeping.
pub struct GreenSpan {
    pub start: usize,
    pub end: usize,
    pub green: NodeOrToken<GreenNode, GreenToken>,
}

/// Wraps a single lexed token — known kind, exact `(start, end)`, exact text
/// — as a `GreenSpan`. No padding needed here: a token's text always exactly
/// fills its own range.
pub fn green_token(kind: SyntaxKind, start: usize, end: usize, text: &str) -> GreenSpan {
    GreenSpan {
        start,
        end,
        green: NodeOrToken::Token(GreenToken::new(kind.into(), text)),
    }
}

/// Combines child `GreenSpan`s into one node covering `start..end`.
///
/// A bottom-up parser only knows a child's own range, not the source text
/// between it and its siblings (that's exactly the whitespace/comments the
/// lexer skips) — so any gap between one child's `end` and the next child's
/// `start` (or between `start`/`end` and the first/last child) is filled
/// with a synthetic `TRIVIA` token of that many space characters. This keeps
/// the built subtree's total text length equal to `end - start`, which is
/// all a caller further up needs to keep combining spans correctly; it does
/// *not* make the tree byte-for-byte lossless yet (the padding is spaces,
/// not the original source text) — that needs the lexer to emit real trivia
/// tokens instead of skipping whitespace/comments.
pub fn green_node(
    kind: SyntaxKind,
    start: usize,
    end: usize,
    children: Vec<GreenSpan>,
) -> GreenSpan {
    let mut items: Vec<NodeOrToken<GreenNode, GreenToken>> =
        Vec::with_capacity(children.len() * 2 + 1);
    let mut cursor = start;
    for child in children {
        if child.start > cursor {
            items.push(NodeOrToken::Token(trivia(child.start - cursor)));
        }
        items.push(child.green);
        cursor = child.end;
    }
    if end > cursor {
        items.push(NodeOrToken::Token(trivia(end - cursor)));
    }
    GreenSpan {
        start,
        end,
        green: NodeOrToken::Node(GreenNode::new(kind.into(), items)),
    }
}

fn trivia(len: usize) -> GreenToken {
    GreenToken::new(SyntaxKind::TRIVIA.into(), &" ".repeat(len))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mimics how a bottom-up grammar (lalrpop) would build `"1 + 2"`: each
    /// production only knows its own children's `(start, end)` and text, not
    /// the source in between, so [`green_node`] has to fill the `" "` gaps
    /// itself as synthetic `TRIVIA` — this is the whole reason it exists.
    #[test]
    fn green_node_pads_gaps_between_children_with_trivia() {
        let one = green_node(
            SyntaxKind::LITERAL_EXPR,
            0,
            1,
            vec![green_token(SyntaxKind::NUMBER, 0, 1, "1")],
        );
        let plus = green_token(SyntaxKind::PLUS, 2, 3, "+");
        let two = green_node(
            SyntaxKind::LITERAL_EXPR,
            4,
            5,
            vec![green_token(SyntaxKind::NUMBER, 4, 5, "2")],
        );
        let binary = green_node(SyntaxKind::BINARY_EXPR, 0, 5, vec![one, plus, two]);

        let node = SyntaxNode::new_root(binary.green.into_node().expect("BINARY_EXPR is a node"));
        assert_eq!(node.text().to_string(), "1 + 2");
    }

    #[test]
    fn green_node_pads_leading_and_trailing_gaps_too() {
        // A node whose own start/end is wider than its children (as if
        // leading/trailing whitespace belonged to the parent) gets padded
        // on both ends, not just between children.
        let ident = green_token(SyntaxKind::IDENT, 1, 2, "x");
        let wrapped = green_node(SyntaxKind::VAR_EXPR, 0, 3, vec![ident]);

        let node = SyntaxNode::new_root(wrapped.green.into_node().expect("node"));
        assert_eq!(node.text().to_string(), " x ");
    }
}
