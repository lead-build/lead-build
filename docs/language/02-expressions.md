# Expressions

Expressions are the heart of the language. Every object, list, tuple, string, integer, path, and even function-like construct is ultimately produced by evaluating an expression. This chapter explains what expressions are, how they behave, and how evaluation proceeds.

The examples in this chapter focus on expressions themselves; the surrounding file header is intentionally left aside for now.

## What Is an Expression?

An expression is any construct that produces a value. This includes:

- Literals such as numbers and strings
- Objects, lists, and tuples
- Operators such as `/` or `++`
- Let-bindings
- References to previously defined names

Every expression evaluates to exactly one value, but that value may be a compound type, such as an object.

## Literal Expressions

The simplest expressions are literal values of any of the types introduced in the previous chapter:

```lead
42
"hello"
true
[1, 2, 3]
{ content = "Hellorld"; }
```

These evaluate directly to themselves. There is no computation involved.

## Computation

An expression can also be computed by using any of the builtin operators:

| Operator      | Function                            |
| ------------- | ----------------------------------- |
| `obj.field`   | Select a field from an object       |
| `func arg`    | Call a function                     |
| `-expr`       | Negate a value                      |
| `obj ? field` | Check if an object has an attribute |
| `lhs ++ rhs`  | List concatenation                  |
| `lhs * rhs`   | Multiplication                      |
| `lhs / rhs`   | Division, or path separator         |
| `lhs - rhs`   | Subtraction                         |
| `lhs + rhs`   | Addition, or string concatenation   |
| `! rhs`       | Logical not                         |
| `lhs // rhs`  | Update / object concatenation       |
| `lhs < rhs`   | Comparison: less than               |
| `lhs <= rhs`  | Comparison: less than or equal      |
| `lhs > rhs`   | Comparison: greater than            |
| `lhs >= rhs`  | Comparison: greater than or equal   |
| `lhs == rhs`  | Comparison: equal to                |
| `lhs != rhs`  | Comparison: not equal to            |
| `lhs && rhs`  | Logical and                         |
| `lhs || rhs`  | Logical or                          |
| `lhs -> rhs`  | Logical implication                 |

Each field in a composite type may also be an expression. For example:

```lead
{
    name = "demo-" + "riscv64";
    version = (1, 0, 0);
    sources = ["main.c", "functions.c"] ++ ["mylib/mylib.c", "mylib/other.c"];
}
```

Just a note: paths in the example above are just strings to illustrate the concept of expressions. In reality, paths use their own type, which will be introduced later.

## Switch expressions

Lead-build also supports a `switch` expression for choosing between a small number of alternatives. The first matching case is used, and an optional `_ => ...` case can provide a default when nothing matches.

```lead
let
  arch = "riscv64";
in
  switch arch {
    "x86_64" => "desktop";
    "riscv64" => "embedded";
    _ => "unknown";
  }
```

Each case is written as `matcher => expression;`. Matchers are compared against the value after `switch` in order, and the first equal case wins.

## Strings

Strings are written with double quotes. They are a basic literal type and can be combined with other strings using `+` for concatenation.

Escape sequences inside strings are written with a backslash. For example:

```lead
"hello\nworld"
"say \"hi\""
```

A newline inside a string literal is represented as `\n`, so the first example evaluates to a string containing a line break between `hello` and `world`.

String interpolation can be used to embed expressions inside a string. Expressions inside `${...}` are evaluated and inserted into the string. For example:

```lead
let
  myvar = "world";
in
  "hello ${myvar}"
```

This evaluates to the concatenation `"hello" + myvar`, so the result is `"hello world"`.

## Let-bindings and variables

Simply computing expressions does not add much unless there is a way to reuse values and give them names. The concept of reusing values is based on the `let <set-statements> in <expression>` construct in lead-build.

A `let`/`in` construct provides a list of variable allocation statements, which are evaluated in order. Each variable allocation statement has the form `destination = expression;`, where the destination is a variable pattern, and `expression` is an expression as described in this chapter, which may even be another `let`/`in` construct.

The variable pattern is discussed further in the chapter on advanced pattern matching. For now, we keep it to the simplest construct: a simple variable name.

The variables can then be used in subsequent set statements and in the final expression, which is the result of the `let ... in ...` construct.

For example:

```lead
let
    name = "demo";
    major_version = 1;
    minor_version = 0;
    patch_version = 0;

    arch = "riscv64";
    version = (major_version, minor_version, patch_version);

    sources = [
        "main.c",
        "function.c"
    ];

    mylib = {
        sources = [
            "mylib/mylib.c",
            "mylib/other.c"
        ];
    };
in
    {
        name = name + "-" + arch;
        version = version;
        sources = sources ++ mylib.sources;
    }
```

This evaluates to:

```lead
{
  name = "demo-riscv64";
  sources = [
    "main.c",
    "function.c",
    "mylib/mylib.c",
    "mylib/other.c",
  ];
  version = (1, 0, 0);
}
```

## Purity and laziness

Each expression has no side effects. The variables are bound during evaluation and cannot be updated later. This aspect is called purity.

That means the evaluation of an expression depends only on the content of the expression and the scope in which it is defined, not on where it is referenced from.

When defining a variable, its value is stored together with the scope in which it was evaluated.

The expression is evaluated the first time its value is actually needed, and the result is then reused. This is an optimization; for example, unused functions in libraries will not be loaded. This is called laziness.

From the user's point of view, this means two things:

- Performance may be improved, especially when using only parts of a larger library.
- Errors, such as undefined variables, may only be detected when evaluating an expression. If the expression is not used in the output, the error may remain undetected.

To fully be able to reuse code, functions are introduced in the next chapter, and later on the concept of includes and multiple files will also be covered.