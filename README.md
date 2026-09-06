# lead build

Lead build is a declarative build system, providing modularity and reusability.

# Documentation

If you want to read about how to use lead-build and lead-lib in your project,
the user guide on https://lead-build.readthedocs.io is for you.

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

If you want to get started with the internals of the lead-build language
interpreter, the [documetnation](docs/index.md) in this reposistory is your
starting point.

# Versioning

From version 1.0.0 and forward, [semantic versioning](https://semver.org/) will
be used.

The interepretation goes as:

- Between major versions (for example 1.x.x and 2.x.x) will not guarantee
  compatibility for build scripts. This will not happen often.
- Between minor versions (for example 1.2.x and 1.3.x) new features can be added
  which does not break compatiblity. No features are removed, or changed in
  behaviour.
- Patch versions is intended only for bug fixes and cosmetic changes, without
  changing of functionality.

Debug output and similar are not considered part of the API and may improve in
any version. This includes the exact format of the `pb -E` output.

Before version 1.0.0, the versioning is focused on stabilization. At the point
of writing, the implementation is quite API stable, but breaking changes may
still occur whenever necessary.

# License

This tool is released under GPL v2