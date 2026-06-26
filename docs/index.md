# Lead Build

Lead Build is a declarative build system for expressing build outputs in terms of their dependencies. Instead of scripting a sequence of commands, Lead Build describes the desired result and how it is composed.

## Why Lead Build

- Declarative: describe *what* to build, not focused on the sequence of commands to build it.
- Modular: build logic can be packaged and reused across projects.
- Reusable: common build patterns can be shared without duplicating file paths or command sequences.

## Example

```lead
|{include, cwd, pb, ...}|
let
    leadlib = include cwd / "lead-lib" / "main.pbb";
    my_lib = include cwd / "mylib" / "main.pbb";
in
leadlib.lang.c.build {
    output = cwd / "myapp";
    builddir = cwd / "build";

    sources = [
        cwd / "src" / "main.c",
        cwd / "src" / "mylib.c",
    ] ++ my_lib.sources;

    includes = [
        cwd / "src";
    ] ++ my_lib.includes;
}
```

## Installation

Currently available as a rust crate.

Run:
```
cargo install lead-build
```

Or check out the git repository at [https://github.com/lead-build/lead-build](https://github.com/lead-build/lead-build)

## Getting started

Start with the language itself, then move on to functions, iteration, and paths.

- [Introduction](language/00-introduction.md)
- [Core Language Concepts](language/01-basics.md)
- [Expressions](language/02-expressions.md)
- [Functions and Pattern Matching](language/03-functions.md)
- [List operations](language/04-list-operations.md)
- [Paths](language/05-paths.md)

## Builds

After the language, learn how to express build rules and produce build graphs:

- [Rules and builds](builds/01-rules-and-builds.md)
- [Abstraction and libraries](builds/02-abstraction.md)

## Next step

After learning the language, the next chapters cover build-specific concepts such as includes, builtins, and project structure.