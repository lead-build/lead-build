# Debugging builtins

Lead Build exposes a `dbg` builtin object for debugging expressions during evaluation.

## `dbg.trace`

Prints the input expression and returns it unchanged.

Syntax:
```lead
dbg.trace expr
```

Behavior:
- Prints `expr` using standard output.
- Returns the same expression value, so it can be inserted into larger expressions without changing behavior.

Example:
```lead
|{dbg, ...}|
let
    x = dbg.trace (1 + 2);
in
x * 10
```

This prints `3` and evaluates to `30`.

## `dbg.break`

Prints the input expression and then raises a debug exception.

Syntax:
```lead
dbg.break expr
```

Behavior:
- Prints `expr` using standard output.
- Raises a `Debug` error with the message `break: <expr>`.

Use `dbg.break` when you want evaluation to stop at a specific point and show the current value.

Example:
```lead
|{dbg, ...}|
let
    x = 1 + 2;
in
dbg.break x
```
