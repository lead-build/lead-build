# Paths

Paths are objects in the language that represent a location in the filesystem. A path value may refer to either a file or a directory.

Paths are typically obtained from builtin functions or other language constructs. The builtin `cwd` represents the directory of the current `.pbb` file and is commonly used as the starting point for path traversal. Every example below includes a file header that brings `cwd` into scope.

## Bound root and purity

To achieve purity also for modules within a build, it build should not have side effects. This also needs to be true for read and generated files. Therefore, it is required that each module, reperesented via the location of the `.pbb` file, that no other files than the ones within the module itself to be access, except if paths are passed explicitly to the module.

To achieved the isolation, `cwd`, which is the path to the directory of the current `.pbb` file, and is locked to the module.

Therefore, paths are called *locked* to a direcotry. Traversing downards and upwards is allowed within a path object, but not outside the *origin*.

Paths are therefore also not possible to create new, but can only be crated from existing path objects by traversal.

Paths can be passed, as all values, as arguments to functions. For example, it is possible to pass the path to a build directory to a function in module which is outside of the module itself, to allow generating files in a build directory.


## Traversal

Use the `/` operator to move down into a directory or file name:

```lead
|{cwd, ...}|
let
  src = cwd / "src";
  main = src / "main.c";
in
  main
```

In this example, `src` is the path one level below `cwd`, and `main` is a path below `src`.

The right-hand side of `/` must be a string representing a child name.

## Upward traversal

You can also move upward using the special segment `".."`:

```lead
|{cwd, ...}|
let
  src = cwd / "src";
  back = src / "..";
in
  back
```

This returns the parent directory of `src`, but not above the original `cwd` origin.


### Locking a path

The builtin `pb` contains a function called `lock` that creates a new path value bound to the same file or directory, but with a fresh root boundary.

```lead
|{cwd, pb, ...}|
let
  locked = pb.lock (cwd / "src");
  parent = locked / "..";
in
  parent
```

In this example, `locked` refers to the same directory as `cwd / "src"`, but its upward traversal is restricted to that path. The example is intended to show that attempting `locked / ".."` does not escape above the locked root and will fail.


## Path remapping

Use the builtin `pb.translate` to rewrite a path by replacing one directory prefix with another. The argument is an object with `input`, `from`, and `to` fields. `input` is the path to rewrite, `from` is either one base directory or a list of base directories that may contain `input`, and `to` is the directory that should replace the matching prefix.

```lead
|{cwd, pb, ...}|
let
  src = cwd / "src" / "main.c";
  remapped = pb.translate {
    input = src,
    from = cwd / "src",
    to = cwd / "build"
  };
in
  remapped
```

This produces a path rooted at `cwd / "build" / "main.c"`, where the `src` prefix has been replaced by `build`.

When `from` is a list, `pb.translate` checks candidates in order and uses the first one that matches.

```lead
|{cwd, pb, ...}|
let
  file = cwd / "lib" / "src" / "main.c";
  remapped = pb.translate {
    input = file,
    from = [cwd / "src", cwd / "lib" / "src"],
    to = cwd / "build"
  };
in
  remapped
```

## File suffix rewriting

File suffixes are rewritten using the `+` and `-` operators, rather than a dedicated builtin. `path + string` appends `string` to the last path element, and `path - string` removes `string` from the end of the last path element (failing if it isn't a suffix of it).

```lead
|{cwd, ...}|
let
  source = cwd / "src" / "main.c";
  object = (source - ".c") + ".o";
in
  object
```

This rewrites `main.c` to `main.o`.

## Rebase paths

Use the builtin `pb.rebase` to express a path under another base path while preserving its relative location.

```lead
|{cwd, pb, ...}|
let
  src = cwd / "src" / "main.c";
  out = pb.rebase {
    path = src,
    base = cwd / "build"
  };
in
  out
```

This is useful when handing paths to tools that need the same relative layout under another root.
