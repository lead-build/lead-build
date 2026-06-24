# Language basics

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