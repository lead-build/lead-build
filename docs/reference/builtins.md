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

### `pb.retype`

Changes the file suffix of a path.

Syntax:
```lead
pb.retype {
  input = path,
  from = string,
  to = string
}
```

- `input`: a path to a file
- `from`: the current file suffix
- `to`: the desired file suffix

Returns a path with the file suffix rewritten from `from` to `to`.

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

The return value of `pb.rule` is callable. Call it with a build argument object to produce a build value:

```lead
compile_rule {
  input = [cwd / "src" / "main.c"];
  output = cwd / "build" / "main.o";
}
```

More information is available in the [builds](../builds/01-rules-and-builds.md) chapter.

### `pb.build`

Low-level constructor for a build value. In normal usage, prefer calling the function returned by `pb.rule`.

```lead
pb.build {
  rule = rule_definition;
  input = [sources...];
  output = output;
}
```

The optional variable `deps` adds any implicit depdendencies to the build. Can be either a single file/build or a list of files/builds.

`rule_definition` is the output of `pb.rule`, and the rest of the variables are defined, except `deps`, from the arguments to the rule definition.

Equivalent preferred form:

```lead
rule_definition {
  input = [sources...];
  output = output;
}
```

More information is available in the [builds](../builds/01-rules-and-builds.md) chapter.

## Debug builtins

The `dbg` object contains helpers for inspecting values while evaluating expressions.

- `dbg.trace`: attempts to eval and print a value, then returns it unchanged.
- `dbg.break`: attempts to eval and print a value, then stops evaluation with a debug error.

See [Debugging builtins](debugging.md) for details and examples.