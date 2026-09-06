# pblang

`pblang` turns pb source text into a syntax tree. It knows nothing about what
the language *means* — no evaluation, no values — only what it *is*
syntactically. That's `pbexpr`'s job (see [pbexpr.md](pbexpr.md)).

The entry point is [`pblang::parse`](../src/pblang.rs), which takes a
`&str` and returns a [`rowan`](https://docs.rs/rowan) `SyntaxNode`.

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
  `green_node` pads those gaps with synthetic `TRIVIA` tokens so a subtree's
  text length always matches its span.

## Why rowan, and its current limits

A `rowan` tree is a red/green syntax tree: same idea as an AST, but every
node stores exactly the source text it covers, so `node.text()` reconstructs
that slice — useful for diagnostics, and eventually for round-tripping and
an auto-formatter.

It isn't byte-exact yet, though: `Tok` still skips whitespace and comments
(`#[logos(skip(..))]`) instead of emitting them as trivia tokens, so the
`TRIVIA` padding `green_node` inserts is synthetic spaces of the right
*length*, not the original bytes. Getting to true lossless round-tripping is
a lexer change (emit real trivia), not a grammar or tree-shape change.

## Finding your way around `SyntaxKind`

`SyntaxKind` in `syntaxtree.rs` is the map of the whole syntax: every
variant's doc comment names what it corresponds to (a `Tok` variant for
tokens, or the old/conceptual node shape for composites). Start there when
you need to know what a piece of syntax looks like as a tree, or where to
add a new one.
