# Language reference

This document provides a reference guide for the Lead language syntax, operators, and language constructs. Use this guide to understand the core language features and their usage.

## Types

Multiple types are available in the langauge. Those available to the user are
listed below.

### Numbers

Numeric literals represent integer.

Example: `42` or `-23`

### Boolean

Boolean values are `true` or `false`. They are used for conditionals and logical expressions.

Example: `true` or `false`

### Strings

Strings are quoted text values. They may contain escaped characters such as `\"` for a quote and `\\` for a backslash. Strings are commonly used for path segments and textual data.

Strings may also contain sequence `${expr}` containing a sub expression. Usually for embedding a variable. This is identical to concatenation of the parts using `+` operator.

Example: `"hellorld"` or `"Hello ${name}!"`

### Objects

Objects are collections of named fields written with `{ key = value; ... }`. They are used for structured data, function arguments, and configuration.

Object keys may be written either as identifiers (`name`) or as quoted strings (`"compiler-flags"`).

Example:
```lead
{
  src = "src";
  out = "build";
}
```

### Lists

Lists are ordered collections written as `[item1, item2, ...]`. They hold a sequence of values of any type.

Items are separated with `,`. Last element may have an optional trailing `,`.

Example:
```lead
[
    "a.c",
    "b.c",
    "c.c"
]
```

### Paths

Paths represent filesystem locations and may refer to either files or directories. Paths are built from builtin values such as `cwd` and are manipulated with the `/` operator.

Example:
```lead
|{cwd, ...}|
let
  src = cwd / "src" / "main.c";
in
  src
```

#### Path locking

Paths are locked to a given directory, and can never traverse outside of the directory.

Failing example: `srcdir / ".."` will not work due to locking.
```lead
|{cwd, ...}|
let
  srcdir = pb.lock (cwd / "src");
  back = srcdir / "..";
in
  back
```


### Builds

Build objects are returned by internal `pb.build` represneting a build operation.

### Build rules

Build rule objects are returned by internal `pb.rule` represneting a build rule.

#### Build rule placeholder object

A placeholder for a variable, available internally in the function defining a `pb.rule`.

## Expressions structure

Following operators are available:

| Operator                     | Precedence | Description                       |
| ---------------------------- | ---------- | --------------------------------- |
| `let ... in ...`             | 1          | let block expression              |
| `|matcher| expr`             | 1          | Function definition               |
| `lhs -> rhs`                 | 2          | Logical implication               |
| `lhs || rhs`                 | 3          | Logical or                        |
| `lhs && rhs`                 | 4          | Logical and                       |
| `lhs == rhs`                 | 5          | Equal                             |
| `lhs != rhs`                 | 5          | Not equal                         |
| `lhs < rhs`                  | 6          | Less than                         |
| `lhs <= rhs`                 | 6          | Less than or equal                |
| `lhs > rhs`                  | 6          | Greater than                      |
| `lhs >= rhs`                 | 6          | Greater than or equal             |
| `lhs // rhs`                 | 7          | Object update/merge               |
| `!expr`                      | 8          | Logical not                       |
| `lhs + rhs`                  | 9          | Addition or string concatentation |
| `lhs - rhs`                  | 9          | Subtraction                       |
| `lhs * rhs`                  | 10         | Multiplication                    |
| `lhs / rhs`                  | 10         | Division or path extension        |
| `lhs ++ rhs`                 | 11         | List concactenation               |
| `lhs ? rhs`                  | 12         | Has attribute                     |
| `-expr`                      | 13         | Numeric negation                  |
| `func arg`                   | 14         | Function call                     |
| `object.ident`               | 15         | attribute selection               |
| `object.{expr}`              | 15         | computed attribute selection      |
| `( func for init .. list )`  | 16         | list fold                         |
| `[ func for iterable ]`      | 16         | list map                          |
| `{ func for iterable }`      | 16         | object map                        |
| `switch expr { ... }`        | 16         | switch expression                 |


### Function defintion

A function is represented as `|matchers| expr`, where matcher is a list of zero or more matchers,
separated by space, providing input variables to the expression `expr`.

Matchers can be any matcher defined in the matcher chapter below.

No matchers in the list is equivalent to no function definition at all. Thus `|| expr` is equal to `expr`

Multiple matchers is equal to multiple nested functions. Thus `|a b| expr` is equal to `|a| |b| expr`.

More information available in [function chapter](../language/03-functions.md) in the documentation.

### Logical "and" and "or"

`&&` and `||` is evaluated lazy.

- Given `rhs && lhs`, and `rhs` is false, then `lhs` is not evaluated.
- Given `rhs || lhs`, and `rhs` is true, then `lhs` is not evaluated.

### List and object iteration

Lists and objects can be iterated over, using the *map* construct.

The map construct has two variants:

- generate a list: `[ func for iterable ]`
- generate an object: `{ func for iterable }`

Both forms also support an optional filter clause:

- generate a filtered list: `[ func for iterable if filter ]`
- generate a filtered object: `{ func for iterable if filter }`

`iterable` can be either a list or object, and `func` is a function that
transform each element in iterable to an output element.

When present, `filter` is evaluated before `func` for each input element.
Only elements where `filter` evaluates to `true` are kept.

For lists, element is passed, or received directly. For objects, each element
is represented to `func` as a tuple of `(key, value)`, both as argument and
return value.

The filter receives the same input representation as the mapper, meaning list
elements for list input and `(key, value)` tuples for object input.

This means four combinations:

| From   | To     | Representation                                                             |
| ------ | ------ | -------------------------------------------------------------------------- |
| List   | List   | `[ |input_val| output_val for input_list ]`                               |
| List   | Object | `{ |input_val| (output_key, output_value) for source_list }`              |
| Object | List   | `[ |(input_key, input_val)| output_val for input_object ]`                |
| Object | Object | `{ |(input_key, input_val)| (output_key, output_value) for source_object }` |

Example with filtering:

```lead
[ |a| a * 2 for [1, 2, 3] if |a| a < 3 ]
```

### Switch expressions

Switch expressions choose the first matching case from a series of value comparisons.

```lead
switch arch {
  "x86_64" => "desktop";
  "riscv64" => "embedded";
  _ => "unknown";
}
```

The general shape is:

```lead
switch expr {
  matcher => result;
  _ => default_result;
}
```

Cases are checked in order. The first matcher equal to the switch value wins, and the optional `_` case acts as a default when no case matches.
