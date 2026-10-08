# pbexpr

`pbexpr` is the language itself: a lazy expression graph, evaluation rules,
pattern matching, and diagnostics. It has no idea what a file path is, what
Ninja is, or how `include` works — it's generic over two things a concrete
environment supplies:

- **`T`** — the value type (numbers, strings, whatever else the language can
  hold), via the `ExprOps<F>`, `ParsableValue` and `Exportable` traits.
- **`F`** — the "file" type used for error locations, via `Referrable`.

[`pbbuild`](pbbuild.md) is what plugs in the real `T` (`Value`) and `F`
(`VirtPath`) and adds everything environment-specific. Keeping that split
means `pbexpr` (and its tests) never has to touch the filesystem or Ninja at
all — see `pbexpr::testvalue` for the toy `T`/`F` used in `pbexpr`'s own
unit tests.

## From syntax tree to expression graph: `parser.rs`

[`pbexpr::parser::parse_str`](../src/pbexpr/parser.rs) is the bridge from
`pblang`: it calls `pblang::parse` to get a `rowan::SyntaxNode`, then walks
it with `pblang::visit::walk_expr`, building an `Expr<T, F>` graph as it
goes. The walk itself (which child is what) belongs to `pblang`; the
`LangVisitor` implementation here, `ExprGenerator`, only decides what each
node *means*: e.g. which `SyntaxKind` token sits between a `BINARY_EXPR`'s
two children determines which `ExprBinOp` it becomes.

## The expression graph: `expr.rs`

`Expr<T, F>` is `Rc<RefCell<ExprStorage<T, F>>>` — a shared, mutable cell
holding an `ExprType<T, F>` plus an optional source `Loc`. `ExprType` is the
actual node enum: `Object`, `List`, `BinOp`, `Let`, `FuncCall`, `Switch`,
`Match`, `Value(T)`, and so on.

The language is lazy: building the graph doesn't evaluate anything.
Evaluation happens through:

- **`resolve()`** — reduces an `Expr` by exactly one "layer" (e.g. a
  `FuncCall` becomes the called function's body with arguments bound), and
  replaces the `Expr`'s own storage in place with the result. It loops until
  the node is something that doesn't need further reduction (a value, an
  object, a list, ...).
- **`eval()`** — calls `resolve()` recursively through a whole graph, so
  every reachable sub-expression gets forced (used to surface *all* errors
  in a structure, not just the first one hit).
- **`value()` / `get_item()` / ...** — convenience accessors that resolve
  just enough to answer one question ("is this a value?", "what's field
  `x`?").

Binding (`bind()`) is how variables become concrete: it clones a subgraph
with a `varspace` (an `ExprSet` = `BTreeMap<StrKey, Expr<T, F>>`) attached,
so a later `resolve()` of a `Var` can look the name up. The result is an
`ExprType::Bind` node, which also has a source form, `bind name = expr; ...
in expr`. That syntax isn't meant for build scripts (it is only noted as
internal in the user reference); it exists so tests can write a bound
expression directly, and so debug output of one is readable.

## Pattern matching: `expr/matcher.rs`

`Matcher<T, F>` is the destructuring pattern used by `let`, function
arguments, and `match` cases — plain identifier, `_`, tuple, list (optionally
only a prefix, with a trailing `...`), or object pattern (with optional
per-field defaults), each optionally with an `@`-alias.

`Matcher::run` takes an `Expr` and produces the `ExprSet` of bindings it
introduces; a value that doesn't fit is an error. `Matcher::try_run` is the
refutable variant used by `match`: a value that doesn't have the matcher's
shape gives `Ok(None)`, so the next case can be tried, while errors from
evaluating the value itself still propagate. Both resolve the value only as
far as needed to know its shape, so elements a matcher doesn't look at stay
unevaluated.

## Diagnostics: `expr/export.rs`

`Exportable` is how any `Expr`/`ExprType`/value renders itself as
pb-syntax-shaped text for error messages and `Display`. It's not meant to be
exact or re-parseable — an `Expr` can be a huge or even cyclic lazy graph
(shared `Rc` structure), so `Printer` (the `Write` sink `export()` writes
through) enforces a depth and item budget: once either runs out, printing
switches to an inline placeholder (e.g. `<list expression>`) instead of
recursing further. This is why it's safe to `Display` an arbitrary `Expr` in
an error message without checking its shape first.

## Errors and locations: `error.rs`

`Error<F>` carries an `ErrorType`, a message, and a backtrace of `Loc<F>`
(source locations, each requiring `F: Referrable` to be printable). Most
`pbexpr` functions return `Result<_, F> = Result<_, Error<F>>`.
