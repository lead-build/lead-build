# Changelog

This page summarizes documentation-relevant language and builtin changes after `v0.4.0`.

## v0.4.1

- Added `pb.rebase` for rebasing path values against another base path.

## v0.4.2

- Added support for object attribute checks with `lhs ? rhs` (`HasAttr`).
- Path arguments were improved in builtin handling to support common path passing workflows.

## v0.4.3

- `pb.translate` now supports multiple `from` base paths by accepting a list and using the first match.
- Operator support and expression resolution were extended and cleaned up, including tuple equality behavior.

## v0.5.0

- `pb.rule` now returns a callable function directly; the standalone `pb.build` builtin was removed. Construct a build by calling the value returned by `pb.rule` with the build arguments.
- Builds support an optional `deps` field for adding implicit dependencies, without them being passed to the underlying rule.

## v0.6.0

- Internal refactors of `PbBuild`/`NinjaFile` (paths for inputs/outputs/deps, rule de-duplication, logging, and statistics). No user-visible language or builtin changes.

## Unreleased

- Added a `null` literal value. `null` is only equal to itself; comparisons with any other value are `false`/`true` for `==`/`!=`.
- Objects now support `==`/`!=`: two objects are equal when they share the same keys and each value is equal.
- Object field names are now interned (`StrKey`) internally; this is not user-visible.
- Added the `ops.zip` builtin for transposing a compound value of compound values (see [Builtin operator functions](builtins.md#builtin-operator-functions)).
- The `pb.retype` builtin was removed. Use the `+`/`-` operators on a path to add/remove a file suffix instead.
- Fold expressions now use `for initial: source` instead of `for initial .. source`, and the `initial` value is optional; when omitted, the first element of the collection seeds the accumulator.
- List/object comprehensions and folds now accept any expression (including `let`, `switch`, etc.) as the source, not just simple expressions.
- Evaluating an expression that is already being evaluated (a dependency cycle) is now detected explicitly and reported as an error.

## Notes

- The crate version at the time of this update is `0.6.0`.
- Use this page as a quick compatibility guide when upgrading from `v0.4.0`.
