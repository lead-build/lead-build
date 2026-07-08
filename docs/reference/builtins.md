# Builtin functions

## Builtin path functions

### `pb.translate`

Rewrites a path by replacing a directory prefix.

Syntax:
```lead
pb.translate {
  input = path,
  from = path,
  to = path
}
```

- `input`: the path to rewrite
- `from`: the base path prefix that must contain `input`
- `to`: the directory to use instead of `from`

Returns a path where the `from` prefix is removed from `input` and replaced by `to`.

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

Creates a build-rule object describing how a build step should be performed. A rule captures the relevant inputs, outputs, and execution behavior for a single build action.

```lead
pb.rule |{input, output, ...}| {
  name = "compile";
  command = ["gcc", "-c", "-o", output, input];
};
```

Note: In `pb.rule`, object matcher defaults (for example, `|{input ? fallback, ...}|`) are not supported.

More information is available in the [builds](../builds/01-rules-and-builds.md) chapter.

### `pb.build`

Creates a build operation from one or more rule definitions. A build object represents an actual build step that can be executed with the given inputs and outputs.

```lead
pb.build {
  rule = rule_definition;
  input = [sources...];
  output = output;
}
```

`rule_definition` is the output of `pb.rule`, and the rest of the variables are defined from the arguments to the rule definition.

More information is available in the [builds](../builds/01-rules-and-builds.md) chapter.

## Debug builtins

The `dbg` object contains helpers for inspecting values while evaluating expressions.

- `dbg.trace`: prints a value and returns it unchanged.
- `dbg.break`: prints a value and stops evaluation with a debug error.

See [Debugging builtins](debugging.md) for details and examples.