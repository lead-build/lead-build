# Builtin functions

## Builtin path functions

### `pb.translate`

Rewrites a path by replacing a directory prefix.

Syntax:
```lead
pb.translate {
  input = path,
  from = path or [path, ...],
  to = path
}
```

- `input`: the path to rewrite
- `from`: one path or a list of candidate base path prefixes that may contain `input`
- `to`: the directory to use instead of `from`

Returns a path where the matching `from` prefix is removed from `input` and replaced by `to`.
If `from` is a list, the first matching prefix is used.

### `pb.rebase`

Converts a Lead path into a path value rebased to another filesystem base path.

Syntax:
```lead
pb.rebase {
  path = path,
  base = path
}
```

- `path`: the path value to convert
- `base`: the base directory to rebase the output against

Returns a path value that keeps the internal relative location of `path`, but expressed under `base`.

### `pb.lock`

Creates a new path value bound to the same file or directory, but with a fresh root boundary, so upward traversal (`..`) cannot escape above it.

Syntax:
```lead
pb.lock path
```

More information is available in the [paths](../language/05-paths.md) chapter.

## Path suffix operators

File suffixes are rewritten using the `+` and `-` operators on a path, rather than a dedicated builtin:

- `path + string` appends `string` to the last path element.
- `path - string` removes `string` from the end of the last path element, and fails if it isn't a suffix.

```lead
let
  source = cwd / "src" / "main.c";
  object = (source - ".c") + ".o";
in
  object
```

## Builtin build functions

### `pb.rule`

Creates a build-rule function describing how a build step should be performed. A rule captures the relevant inputs, outputs, and execution behavior for a single build action.

```lead
pb.rule |{input, output, ...}| {
  name = "compile";
  command = ["gcc", "-c", "-o", output, input];
};
```

Note: In `pb.rule`, object matcher defaults (for example, `|{input ? fallback, ...}|`) are not supported.

The return value of `pb.rule` is callable. Call it with a build argument object to produce a build value; this is the only way to construct a build value, there is no separate `pb.build` builtin:

```lead
compile_rule {
  input = [cwd / "src" / "main.c"];
  output = cwd / "build" / "main.o";
}
```

The optional variable `deps` adds any implicit dependencies to the build, and is not passed to the rule itself. It can be either a single file/build or a list of files/builds.

More information is available in the [builds](../builds/01-rules-and-builds.md) chapter.

## Builtin operator functions

### `ops.zip`

Transposes a compound value of compound values, pairing up elements that share the same index/key.

Syntax:
```lead
ops.zip outer
```

- `outer`: a list, tuple, or object whose elements are themselves lists, tuples, or objects.

All elements of `outer` must decompose into the same compound type (all lists, all tuples, or all objects); mixing types is an error, as is passing an empty `outer`.

`ops.zip` regroups the elements by their inner index/key: the result is a value of the inner compound type, where each entry is a value of the outer compound type collecting the corresponding elements from every inner value. Any missing index in a list or tuple is padded with `null`.

```lead
ops.zip [[1, 2], [3, 4], [5, 6]]
```

This evaluates to:

```lead
[[1, 3, 5], [2, 4, 6]]
```

## Debug builtins

The `dbg` object contains helpers for inspecting values while evaluating expressions.

- `dbg.trace`: attempts to eval and print a value, then returns it unchanged.
- `dbg.break`: attempts to eval and print a value, then stops evaluation with a debug error.

See [Debugging builtins](debugging.md) for details and examples.