# List operations

Collection iteration lets you transform lists and objects without writing explicit loops. The language provides a concise, expression-oriented syntax for mapping over each element or reducing a collection to a single value.

## Iteration

The language also supports a map-like iteration form for transforming collections. This is useful when you want to apply the same transformation to every item in a list or object and collect the results.

### General shape

The basic idea is:

```lead
[ |pattern| expression for source ]
{ |pattern| expression for source }
[ |pattern| expression for source if |pattern| predicate ]
{ |pattern| expression for source if |pattern| predicate }
```

- Square brackets `[...]` produce a list.
- Curly braces `{...}` produce an object.
- The `pattern` is matched against each item in the source.
- The `expression` is evaluated once per item, using the matched values.
- The optional `if` clause filters input items before mapping.

### Filtering during map

Map expressions support an optional filter written after the source expression:

```lead
[ mapper for source if predicate ]
{ mapper for source if predicate }
```

The filter is a function that receives the same input item as the mapper:

- For list input, it receives each list element.
- For object input, it receives a `(key, value)` tuple.

Only items where the predicate evaluates to `true` are passed to the mapper.

### Filter a list map

```lead
[ |a| a * 2 for [1, 2, 3, 4] if |a| a < 3 ]
```

This evaluates to:

```lead
[2, 4]
```

### Filter an object map

```lead
{ |(k, v)| (k, v * 10) for {a=1; b=2; c=3;} if |(k, v)| v >= 2 }
```

This evaluates to:

```lead
{b=20; c=30;}
```

### Map over a list, returning a list

```lead
[ |v| v + 3 for input_list ]
```

This applies the function `|v| v + 3` to each element of `input_list`. The result is a new list whose values are each transformed by the expression.

### Map over an object, returning a list

```lead
[ |(k, v)| v + 3 for input_object ]
```

When iterating over an object, each item is exposed as a pair of `(key, value)`. This form transforms each value and collects the results into a list.

### Map over a list, returning an object

```lead
{ |v| (v, "value " + v) for list }
```

This form turns each list element into a key/value pair. The first element of the pair becomes the object key, and the second becomes the object value.

### Map over an object, returning an object

```lead
{ |(k, v)| (k, v + 3) for object }
```

This iterates over an object, transforms each `(key, value)` pair, and builds a new object from the transformed pairs. The result must again be a `(key, value)` pair for each entry.

## Fold

A fold reduces a collection to a single value by repeatedly combining an accumulator with each item. This is useful for things like summing values, concatenating strings, or building a more complex result step by step.

### General shape

The general form is:

```lead
(|accumulator field| expression for initial .. source)
```

- `initial` is the starting value of the accumulator.
- `accumulator` is the value from the previous step.
- `field` is the current item from the collection.
- The `expression` returns the next accumulator value.

For example:

```lead
(|prev field| prev * 10 + field for 7 .. [1, 2, 3])
```

This evaluates as:

```lead
(((7 * 10 + 1) * 10 + 2) * 10 + 3)
```

and results in:

```lead
7123
```

### Use cases

Fold is a general-purpose building block used to implement many helper functions and utilities. Common uses include:

- Reducing numeric sequences (sum, product, min/max).
- Concatenating strings or lists.
- Accumulating statistics or constructing maps/objects from a collection.
- Implementing stateful traversals and other higher-level helpers (e.g., group-by, partition, scan).

Because fold exposes both the accumulated state and the current item, it is well suited for composing small reusable functions that operate over collections.

### Fold examples: sum and product

```lead
# sum: adds all numbers in a list
let
  sum = |lst| (|acc v| acc + v for 0 .. lst);
in
  sum [1, 2, 3, 4]    # => 10
```

```lead
# product: multiplies all numbers in a list
let
  product = |lst| (|acc v| acc * v for 1 .. lst);
in
  product [2, 3, 4]   # => 24
```

