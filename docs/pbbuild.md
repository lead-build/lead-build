# pbbuild

`pbbuild` is everything specific to *this* build system: it supplies the
concrete value and file types [`pbexpr`](pbexpr.md) is generic over, adds
the actual build-graph and Ninja-output logic, and exposes the builtin
functions build scripts call.

## The concrete value and file types

- **`value::Value`** — the concrete `T` for `pbexpr`. Besides plain
  `Int`/`String`/`Bool`, it has `Path` (a `VirtPath` plus the `PbBuild`s it
  depends on — see below) and `BuildVar`/`BuildConcat` for values that
  aren't known until Ninja variable expansion time (e.g. concatenating a
  path with `$in`).
- **`path::VirtPath`** — the concrete `F` for `pbexpr` (implements
  `Referrable`, so it's what error locations print), and also lead-build's
  path type generally: paths are rooted (relative to a project root, an
  output directory, ...) rather than plain filesystem paths, which is what
  lets `include`d files and generated outputs compose correctly regardless
  of where they physically live.

## Entry point: `context::LangContext`

`LangContext` is what a caller (the `pb` binary, or an embedder) actually
holds. `LangContext::read_file` parses one file with `pbexpr::parse_str`;
`LangContext::include` is what the language's own `include` builtin calls —
it reads the target file and calls it as a function, passing a scope that
has `cwd`, a bound `include` (so nested includes resolve relative to the
right context), and optional `args` already inserted. Builtins are attached
here too (`add_builtin`), on top of the defaults from `builtins::get_builtins`.

## Builtins

`builtins::get_builtins` assembles the builtin namespaces exposed to build
scripts as `Expr` values (each implementing `pbexpr::ExprBuiltin`):

- **`builtins/pb.rs`** — the build-graph primitives: `rule`, `build`,
  `lock`, `translate`, `rebase`. `rule`/`build` are what actually construct
  `PbBuildRule`/`PbBuild` (defined in `pbbuild.rs` itself) and attach them
  as the `depends` of a `Value::Path`.
- **`builtins/ops.rs`** — general-purpose helpers (`zip`, ...) that don't
  need the build graph.
- **`builtins/dbg.rs`** — `trace`/`break`, for debugging build scripts
  themselves.

## The build graph: `PbBuild` / `PbBuildRule` (in `pbbuild.rs`)

A `PbBuildRule` is a Ninja rule (a command template); a `PbBuild` is one use
of it (inputs, outputs, deps, per-build variables), plus the `PbBuild`s it
depends on. These accumulate as ordinary data inside `Value::Path.depends`
while a build script evaluates — nothing touches Ninja yet at this point.
`PbBuild::populate_ninja_file` is what finally writes one build graph node
(and, recursively, everything it depends on) into a `NinjaFile`.

## Turning a result into `build.ninja`

- **`ninjaexpr::add_expr_to_ninjafile`** — walks a fully-evaluated top-level
  `Expr` (typically an object of named outputs), and for every `Value::Path`
  it finds, calls `populate_ninja_file` on its dependencies and adds a Ninja
  alias for the field name.
- **`ninjawriter.rs`** — the in-memory Ninja file model (`NinjaFile`,
  `NinjaRule`, `NinjaBuild`, `NinjaArg`, ...) and its text serialization.
  This is the only place that knows Ninja file syntax.
- **`stats.rs`** — `NinjaFileStats`, simple bookkeeping (rule/output counts)
  kept alongside a `NinjaFile`, used for diagnostics/logging.
