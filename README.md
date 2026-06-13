# lead build

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

# Versioning

From version 1.0.0 and forward, [semantic versioning](https://semver.org/) will
be used.

- Between major versions (for example 1.x.x and 2.x.x) will not guarantee
  compatibility for build scripts. This will not happen often.
- Between minor versions (for example 1.2.x and 1.3.x) new features can be added
  which does not break compatiblity. No features are removed, or changed in
  behaviour.
- Patch versions is intended only for bug fixes and cosmetic changes, without
  changing of functionality.

Before version 1.0.0, the versioning is as follows:
- Versions 0.0.x - non-working, but proof of concept of parts
- Versions 0.1.x - working version, but rapidly updating. Please try out and
  give feedback. But no guarantees in stability.
- Versions 0.2.x to 0.9.x - Syntax is stabilizing, but may change based on
  feedback. Don't expect compatilbity between versions, but changes should not
  be major. Feedback is appreciated.

# Status of the project

The project is still in early development.

# License

This tool is released under GPL v2