# Lead Build

Something about declarative builds

```
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
Lead build is a declarative build system, providing modularity and reusability.

## Declarative

A classic script defines a build as a sequence of operations to reach a goal.
The concept of "sequence of operations" means it describes _how_ to produce the
output, rather than what the _goal_.

In a declarative build system like lead, the goal is in focus, how to combine
it from its _dependencies_, and only its dependencies.

In lead, each _intermediate_ node is treated as a varaible, not the file it
produces, in contrast to "gnu make", which also means it can be prameterized
into functions.

## Modularity

Since intermediate build steps can be paramterized, it is possible to make
modules reusable for different targets.

For example, a module producing an `.o` file, with some headers, can fully own
its build description, but still be parameterized on which compiler to use,
where the intermediate files should be placed, and even which compiler and
architecture should be the goal, where the project using the library only needs
to call the librarys lead `.pbb` script.

## Reusability

With the modularity and paramterization in mind, it is possible to define
conventions of how to call libraries and shared code, to be able to reuse
modules easily between projects, without needing to specify paths.

# Name

The goal is to make "Pure builds", and thus the tool `pb`. Which naturally means
results the project "lead"

## Getting started

Before getting started with builds, lets learn the basics of the language. It's
more than just build rules.

Recommended path is to start with language basics:

- [Language basics](language/01-basics.md) - data types, structures and variables
- [Functions](language/02-functions.md) - learn about functions, and how to reuse
  patterns
- [List and object iteration](language/03-list-compherension.md) - List
  comperensions, iterating over objects, and folding lists. The core of managing
  lists.
- [Files, includes and builtins](langauge/04-buildins.md) - how to split your code
  into multiple files, for reusability.

## Building

Lets look at step