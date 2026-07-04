# Core Language Concepts

Before diving into the language in depth, we need to establish the fundamental building blocks: basic types and composite types such as objects, lists, and tuples.

In the previous chapter, you could read how to evaluate the code, which is helpful in the chapters below.

You might have noticed the leading line, `|{ ... }|`. For now, you can safely ignore it and just keep it as the first line in your `.pbb` file throughout the first chapters. We'll return to its purpose later, once we have introduced builtins and the file header more fully. What matters here is the structure that follows. It is part of the file header and can be updated later.

## Basic types

There are a few basic types that can be written in lead-build:

- Strings - represented by a string surrounded by quotes, for example `"hello"`, where letters can be escaped. Strings may also contain substitutions. More on those later.
- Integers - a simple number containing digits `0`-`9` and an optional prefixed minus sign.
- Booleans - the words `true` or `false`.

Besides the basic types listed above, there are other types that cannot be written explicitly, but are returned by builtin functions. These will be covered in future chapters.

Those include:
- Paths - representing a path in the file system
- Build rules - an instruction for how to perform a build operation
- Builds - representing a build that uses a build rule to transform input files into output files.

## Comments

Lead-build supports line comments that start with `#` and continue to the end of the line.

```lead
# This is a full-line comment
{
    name = "demo"; # This is a trailing comment
}
```

If `#` appears inside a string literal, it is treated as normal string content, not as a comment start.

```lead
"value#part"
```

## Objects

Objects are the primary data structure in the language. They group named fields into a single value. An object is written using braces `{ ... }`, with each field defined as:

```lead
field_name = expression;
```

The field name can be either an identifier or a quoted string. This is useful when a key contains characters that are not valid in identifiers.

```lead
{
    "compiler-flags" = ["-O2", "-Wall"];
    normal_name = 1;
}
```

Here is a simple example:

```lead
{
    name = "value as string";
    int_field = 131;
    another_object = { something = 123; };
}
```

Objects can contain:

- Primitive values (strings, integers, booleans)
- Other objects
- Lists
- Tuples
- Computed expressions

Each field ends with a semicolon. Fields are unordered; later chapters will explain how evaluation works.

## Lists

A list is an ordered collection of values. Lists are written using square brackets (`[` / `]`), and values are separated with a comma (`,`):

```lead
[1, 2, 3]
```

Lists can contain any type of expression, including objects:

```lead
[
    1,
    "hello",
    { x = 10; y = 20; }
]
```

## Tuples

Tuples are lightweight, fixed-size groupings of values. They are written using parentheses:

```lead
(123, 23, 12)
```

Unlike lists, tuples are not intended for iteration or dynamic manipulation. They are best used for:

- Returning multiple values
- Representing values that are grouped together.
- Compact data grouping

Tuples are also used to represent keys and values when iterating over objects. More on that later.

Tuples can also contain any expression:

```lead
("x", 42, { nested = true; })
```
