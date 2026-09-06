# lead-build developer documentation

> This is documentation for developing lead-build itself. If you're looking
> for how to *use* lead-build or lead-lib to write build scripts, that's the
> user manual, at https://lead-build.readthedocs.io.

## What this is

lead-build compiles a declarative build language (`.pbb` files) into a Ninja
build file. Source text goes through three layers to get there:

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

## Modules

| Module | Page | Role |
| --- | --- | --- |
| `pblang` | [pblang.md](pblang.md) | Turns source text into a syntax tree (lexer + grammar + CST) |
| `pbexpr` | [pbexpr.md](pbexpr.md) | The language runtime: lazy expressions, evaluation, matchers, diagnostics |
| `pbbuild` | [pbbuild.md](pbbuild.md) | Everything specific to this build system: `Value`, paths, builtins, Ninja file generation |
| `strkey` | — | `StrKey`: a small interned-string type used everywhere as a cheap, `Copy` map key |
| `bin/pb` | — | The `pb` CLI binary: wires a `pbbuild::LangContext` up to a root file and writes `build.ninja` |

## Where does new code go?

- New syntax (an operator, a new expression form)? `pblang` — lexer, grammar,
  `SyntaxKind` — then teach `pbexpr`'s parser how to turn the new tree shape
  into an expression.
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
