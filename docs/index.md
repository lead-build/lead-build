# Lead Build

Lead Build is a declarative build system for expressing build outputs in terms of their dependencies. A path value may refer to either a file or a directory. Instead of scripting a sequence of commands, Lead Build describes the desired result and how it is composed.

## Why Lead Build

- Declarative: describe *what* to build, not *how* to build it.
- Modular: build logic can be packaged and reused across projects.
- Reusable: common build patterns can be shared without duplicating file paths or command sequences.

## Comparison with other build systems

The classic `make` tool is initially easy and powerful for:

- having a single target binary
- a set of source files
- a global set of compiler flags

This is often true for smaller projects that compile natively. For example:

```make
APP=my_app

SRCS=\
    src/main.c \
    src/mylib.c

OBJS=$(patsubst src/%.c,obj/%.o,$(SRCS))

obj/%.o: src/%.c
    @mkdir -p $(@D)
    gcc -c -o $@ $<

$(APP): $(OBJS)
    gcc -o $@ $^
```

However, what happens if you also want to:

- compile one variant for debugging with `-O0 -g`
- compile another variant for release with `-O3`
- compile tests with different libraries
- generate source files such as protocol buffers or parser grammars

A bigger issue appears when writing for embedded systems and multiple targets:

- compiler changes between targets, but source files are *mostly* the same
- libraries for the same architecture can be reused, but
    - linking may differ depending on memory architecture
    - different board support packages may be required
- digital twins and simulators may use totally different compilers

Any build system that relies on global state - for example, one that assumes a global list of source files - becomes problematic when the set of inputs is target-dependent.

This is why declarative builds matter: each build is *pure* - it depends on its input parameters and *only* its input parameters, even if it at first glance looks a bit more complicated.

### Reusability

Using a declarative language to define builds also enables reuse.

Imagine a library, for example something small - an embedded implementation of `printf` - or something bigger - an IPv6 network stack.

For the integrator, you want to:

1. download the library, possibly using git submodules
2. add it to the build system, making:
    - its headers available in the include path
    - sources added to the build, possibly via intermediate `.o` files
    - compilation use the correct per-target flags
    - and not worry about its internal structure beyond the public API
3. add it to the targets you want to include, but possibly not all of them
4. build

This is possible if the library specifies its build definition in *lead-build* format and uses conventions for exposing the build.

Since the build format of the library is defined by the library itself, the library can be upgraded internally without changing how it is integrated, as long as its public API stays compatible.

For this, [lead-lib](https://lead-lib.readthedocs.io/) was created, which defines build structures and conventions for integration.

## Example

And a small example of how a lead-build can look:

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

Currently available as a Rust crate.

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