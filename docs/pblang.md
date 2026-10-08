# pblang

`pblang` turns Lead source text (`.pbb`) into a syntax tree. It knows nothing about what
the language *means* — no evaluation, no values — only what it *is*
syntactically. That's `pbexpr`'s job (see [pbexpr.md](pbexpr.md)).

The entry point is [`pblang::parse`](../src/pblang/parser.rs), which takes a
`&str` and returns a [`rowan`](https://docs.rs/rowan) `SyntaxNode`, together
with any errors the parser recovered from.

## The pipeline: lexer → grammar → tree

```
&str ──[lexer.rs, logos]──▶ token stream ──[grammar.lalrpop, lalrpop]──▶ SyntaxNode (rowan)
```

- **`lexer.rs`** — a [`logos`](https://docs.rs/logos)-derived tokenizer
  (`Tok`). It's *modal*: string literals switch the lexer into a separate
  token set (`StrTok`) via `logos::Lexer::morph`, so that `${...}`
  interpolation inside a string is lexed as real, separate tokens
  (`StringEmbedStart`, the embedded expression's own tokens, `StringEmbedEnd`)
  rather than being baked into one opaque string token. A small depth
  counter tracks nested `{`/`}` so a brace inside an interpolated expression
  (e.g. an object literal) isn't mistaken for the string's own closing
  brace.
- **`grammar.lalrpop`** — a [`lalrpop`](https://lalrpop.github.io/lalrpop/)
  grammar (compiled to `pblang::grammar` by `build.rs`). It's a bottom-up
  parser: every rule receives its children already reduced to values and
  combines them — there's no top-down "start this node, recurse, finish it"
  step.
- **`syntaxtree.rs`** — defines `SyntaxKind` (every token and node kind the
  grammar can produce) and the `rowan::Language` boilerplate it needs, plus
  the two functions the grammar actually calls to build the tree:
  `green_token` (wrap one lexed token) and `green_node` (combine child
  `GreenSpan`s into a node). Because the grammar is bottom-up, a rule only
  knows its children's own `(start, end)` ranges, not the source text
  between them (that's exactly the whitespace/comments the lexer skips) —
  `green_node` fills those gaps with `TRIVIA` tokens holding the real source
  bytes, so a subtree's text always matches its span.

## Why rowan, and its current limits

A `rowan` tree is a red/green syntax tree: same idea as an AST, but every
node stores exactly the source text it covers, so `node.text()` reconstructs
that slice byte for byte, whitespace and comments included. That is what the
formatter and the language server build on.

Two limits remain:

- `Tok` still skips whitespace and comments (`#[logos(skip(..))]`) rather than
  emitting them as tokens of their own. They only exist in the tree as the
  `TRIVIA` gaps `green_node` fills in, one token per gap, not broken down into
  individual whitespace runs and comments.
- Trivia *outside* the outermost parsed node — for example a comment before
  the very first token of a file — isn't covered, since there is no node
  whose span it could be attributed to.

## Walking the tree: `visit.rs`

`visit.rs` is the one place that knows how each `SyntaxKind` node is shaped:
which child is the body, which children are a list of bindings, how a `match`
case splits into matcher and result. `walk_expr` and `walk_matcher` do that
extraction, and hand each child to the matching method of the `LangVisitor`
trait as a not-yet-visited handle. The implementor decides which handles to
visit, and with what top-down state.

Every consumer of the tree is a `LangVisitor`: `pbexpr`'s parser lowers the
tree into its expression graph, and `pbls` derives its semantic views
(scopes, symbols) from it. None of them touches `SyntaxNode::children()`
directly, so a change in the grammar's shape is made in one place.

## The formatter: `fmt.rs`

`fmt.rs` pretty-prints a syntax tree back to source text, using the
[`pretty`](https://docs.rs/pretty) crate for width-aware layout. It works on
the tree alone, never evaluating anything, and keeps comments by carrying
over the `TRIVIA` gaps. It is used by the `pbfmt` binary and by the language
server's formatting request.

## Finding your way around `SyntaxKind`

`SyntaxKind` in `syntaxtree.rs` is the map of the whole syntax: every
variant's doc comment names what it corresponds to (a `Tok` variant for
tokens, or the old/conceptual node shape for composites). Start there when
you need to know what a piece of syntax looks like as a tree, or where to
add a new one.

