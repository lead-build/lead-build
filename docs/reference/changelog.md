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

## Notes

- The crate version at the time of this update is `0.4.3`.
- Use this page as a quick compatibility guide when upgrading from `v0.4.0`.
