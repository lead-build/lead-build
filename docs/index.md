# lead-build developer documentation

> This is documentation for developing lead-build itself. If you're looking
> for how to *use* lead-build or lead-lib to write build scripts, that's the
> user manual, at https://lead.readthedocs.io.

## What this is

lead-build is the implementation of the Lead language, the declarative build
language of the Lead Build System. It compiles `.pbb` files into a Ninja build
file. Source text goes through three layers to get there:

```
source text (.pbb)
      │
      ▼
  pblang    lexing + parsing → a syntax tree
      │
      ▼
  pbexpr    the language itself: a lazy expression graph, evaluation, matchers
      │
      ▼
  pbbuild   the execution environment: paths, builtins, Ninja output
      │
      ▼
  build.ninja
```

Each layer only depends on the one before it in this list. `pblang` doesn't
know `pbexpr` exists. `pbexpr` doesn't know `pbbuild`, Ninja, or the
filesystem exist — it's generic over a value type and a "file/location" type,
which `pbbuild` is the one to supply. See each module's own page for why that
split exists and what it buys.

Two more consumers sit beside that pipeline, both working directly on
`pblang`'s syntax tree without evaluating anything: the formatter
(`pblang::fmt`, the `pbfmt` binary) and the language server (`pbls`).

## Modules

| Module | Page | Role |
| --- | --- | --- |
| `pblang` | [pblang.md](pblang.md) | Turns source text into a syntax tree (lexer + grammar + CST), plus the shared tree walk and the formatter |
| `pbexpr` | [pbexpr.md](pbexpr.md) | The language runtime: lazy expressions, evaluation, matchers, diagnostics |
| `pbbuild` | [pbbuild.md](pbbuild.md) | Everything specific to this build system: `Value`, paths, builtins, Ninja file generation |
| `pbls` | — | The language server: LSP features (diagnostics, semantic tokens, goto definition, references, rename, symbols, folding, formatting) derived from the syntax tree |
| `strkey` | — | `StrKey`: a small interned-string type used everywhere as a cheap, `Copy` map key |
| `bin/pb` | — | The `pb` CLI binary: wires a `pbbuild::LangContext` up to a root file and writes `build.ninja` |
| `bin/pbfmt` | — | The `pbfmt` CLI binary: formats or lints `.pbb` files using `pblang::fmt` |
| `bin/pbls` | — | The `pbls` CLI binary: starts the language server over stdio or a TCP socket |

## Where does new code go?

- New syntax (an operator, a new expression form)? `pblang` — lexer, grammar,
  `SyntaxKind` — then describe the new node's shape in `pblang::visit`
  (`walk_expr`/`walk_matcher` and a `LangVisitor` method). The compiler then
  points out every consumer that has to handle it: `pbexpr`'s parser, which
  turns the new tree shape into an expression, and the visitors in `pbls`.
  Also check whether the formatter (`pblang::fmt`) needs a layout rule for it.
- A new language semantic (how an operator evaluates, a behavior that
  doesn't need the filesystem or Ninja)? `pbexpr`.
- Anything environment-specific (a new builtin function, path handling,
  Ninja output, the filesystem)? `pbbuild`.

## Tests

- Unit tests live with the code they test, in `#[cfg(test)] mod tests` at
  the bottom of the relevant file.
- `tests/fixtures/parsing_cases/` — `.pbb` sources exercised by
  `tests/parsing_fixture_runner.rs`, which parses and evaluates them
  (`ok_*` cases must succeed, `err_*` must fail).
- `tests/fixtures/ninja_cases/` — `.pbb` sources exercised by
  `tests/ninja_fixture_runner.rs`, which runs a build all the way through to
  a generated Ninja file and checks it against `expect.toml` in the same
  directory.

Adding a fixture directory with a `main.pbb` (and `expect.toml` for the
Ninja cases) is usually enough; no runner code changes needed.
