# lead-build

The Lead Build System, or just Lead, is a declarative build system, providing
modularity and reusability.

This repository contains `lead-build`: the Lead language, and the tools for
working with it:

- `pb` - generates [Ninja](https://ninja-build.org/) build files from a build
  description
- `pbfmt` - formatter and linter for build descriptions
- `pbls` - language server, for editor integration

It is normally used together with
[lead-lib](https://github.com/lead-build/lead-lib), the library that provides
the conventions and language support to build real projects.

# Installation

```sh
cargo install lead-build
```

# Documentation

If you want to read about how to use lead-build and lead-lib in your project,
the user guide on https://lead.readthedocs.io is for you.

There you will learn how this works:

```pbb
|{ cwd, include, ... }|
let
    lib = include "${cwd}/lead-lib/lead-lib.pbb" { };

    my_app = lib.merge [
        lib.lang.c.mod {
            src = [ "${cwd}/src/main.c" ];
            inc = [ "${cwd}/src/" ];
        },
        lib.lang.rust.mod {
            name = "myrustlib";
            dir = "${cwd}/myrustlib";
        },
    ];
in
lib.build [
    lib.lang.config.simple "${cwd}",
    lib.lang.c.app_build "my_app",
    my_app,
]
```

If you want to get started with the internals of the Lead language
interpreter, the [documentation](docs/index.md) in this repository is your
starting point.

# Versioning

From version 1.0.0 and forward, [semantic versioning](https://semver.org/) will
be used.

The interpretation is:

- Major versions (for example 1.x.x and 2.x.x) do not guarantee compatibility
  for build scripts between them. This will not happen often.
- Minor versions (for example 1.2.x and 1.3.x) can add new features that do not
  break compatibility. No features are removed, or changed in behaviour.
- Patch versions are intended only for bug fixes and cosmetic changes, without
  changes in functionality.

Debug output and similar are not considered part of the API and may improve in
any version. This includes the exact format of the `pb -E` output.

Before version 1.0.0, the versioning is focused on stabilization. At the time
of writing, the implementation is quite API stable, but breaking changes may
still occur whenever necessary.

# License

This tool is released under GPL v2.
