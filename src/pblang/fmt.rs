//! The pb auto-formatter (`pbfmt`). Kept as its own submodule, separate
//! from the lexer/grammar/`syntaxtree`, since those are shared with parsing
//! proper — formatting logic shouldn't leak into them.
//!
//! [`format_tree`] takes a parsed [`SyntaxNode`] and returns a freshly
//! reformatted one. It works in three steps:
//! 1. Walk the tree and build a [`pretty`] `Doc` — a small combinator tree
//!    (`text`, `line`, `nest`, `group`, ...) describing *how* each
//!    construct is allowed to lay out, not the final text. Leaves are the
//!    verbatim source text of real tokens (and of whole `STRING_LIT`
//!    subtrees, treated as one opaque atom — see below).
//! 2. Render that `Doc` to a plain `String` at a fixed width. This is where
//!    `pretty`'s Wadler-style algorithm decides, per `group`, whether it
//!    fits on the current line or needs to break — the actual reason to use
//!    it instead of a fixed per-`SyntaxKind` spacing table is that the same
//!    "try flat, else break with one item per line" rule then applies
//!    uniformly regardless of how long a list/tuple/etc. happens to be.
//! 3. Re-parse that string with [`super::parse`]. Since the rendered text
//!    just has ordinary whitespace wherever a break landed, this reuses the
//!    lexer/grammar's own `TRIVIA`-from-real-bytes mechanism to rebuild a
//!    proper tree, rather than us needing to splice green nodes back
//!    together by hand.
//!
//! `pretty::Doc` only knows about strings, not rowan — it never sees a
//! `SyntaxNode`/`GreenNode` directly, which is why step 3 exists at all.
//!
//! Two things are deliberately *not* reformatted:
//! - `STRING_LIT` nodes are copied through byte-for-byte, interpolation
//!   included. Reformatting the inside of a string isn't something a
//!   formatter should second-guess. (One known rough edge: this grammar
//!   allows a literal newline inside a string chunk, so a `STRING_LIT` atom
//!   can itself span multiple lines; `pretty`'s column tracking assumes
//!   single-line atoms, so layout decisions immediately after such a string
//!   can be suboptimal. The text itself is still always correct — this
//!   only risks a wonky nearby line break, not corrupted output.)
//! - Comments are preserved (recovered from the original trivia and
//!   re-emitted on their own line, forcing a break at that point — a
//!   comment can never be part of a single-line rendering), but *only*
//!   comments — blank-line layout and exact spacing are not preserved.

use super::syntaxtree::{SyntaxKind, SyntaxNode, SyntaxToken};
use pretty::RcDoc;
use rowan::NodeOrToken;

type Doc = RcDoc<'static, ()>;

/// Target line width `pretty` tries to fit a `group` within.
const WIDTH: usize = 100;
/// Spaces per indent level, matching the rest of the codebase's style.
const INDENT: isize = 4;

/// Rebuilds `tree` with canonical formatting. See the module docs.
pub fn format_tree(tree: &SyntaxNode) -> SyntaxNode {
    let piece = format_node(tree.clone());
    let rendered = piece.doc.pretty(WIDTH).to_string();
    super::parse(&rendered).unwrap_or_else(|error| {
        panic!(
            "pblang::fmt produced text that failed to re-parse (this is a formatter bug, \
             not a problem with the input): {error}\n\n--- formatted output ---\n{rendered}"
        )
    })
}

/// A freshly-built `Doc`, plus whether it wants to always start on its own
/// line (used for `let`/`bind`, which read poorly glued onto whatever
/// precedes them) rather than being placed inline by the parent.
struct Piece {
    doc: Doc,
    own_line: bool,
}

enum Elem {
    Node(SyntaxNode),
    Token(SyntaxToken),
}

impl Elem {
    fn kind(&self) -> SyntaxKind {
        match self {
            Elem::Node(n) => n.kind(),
            Elem::Token(t) => t.kind(),
        }
    }
}

/// Non-trivia children of `node`, plus the raw trivia text found between
/// them: `gaps[i]` is the text between `elems[i - 1]` and `elems[i]`
/// (`gaps[0]` is before the first element). There's no meaningful trailing
/// gap after the last element to also capture: every node's own span ends
/// exactly at its last real child's end (see `syntaxtree::green_node`), so
/// nothing is ever lost by not tracking one.
fn real_children(node: &SyntaxNode) -> (Vec<String>, Vec<Elem>) {
    let mut gaps = vec![String::new()];
    let mut elems = Vec::new();
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Token(t) if t.kind() == SyntaxKind::TRIVIA => {
                gaps.last_mut().unwrap().push_str(t.text());
            }
            NodeOrToken::Token(t) => {
                elems.push(Elem::Token(t));
                gaps.push(String::new());
            }
            NodeOrToken::Node(n) => {
                elems.push(Elem::Node(n));
                gaps.push(String::new());
            }
        }
    }
    (gaps, elems)
}

/// Pulls `#`-comments out of a raw trivia blob, trimmed, in order. This is
/// the only thing recovered from the original trivia text — everything
/// else about it (exact whitespace, blank lines) is discarded.
fn comments_in(gap: &str) -> Vec<String> {
    gap.lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// The `Doc` to place between two adjacent pieces, given what `default`
/// would otherwise be. If `gap`'s original trivia held any comments, they
/// always win: each gets emitted on its own line via `hardline`, followed
/// by one more `hardline` before whatever comes next — regardless of what
/// `default` was — because a comment runs to the end of its line, so
/// *anything* placed right after one without a real line break would
/// silently become part of the comment once re-parsed.
fn gap_doc(gap: &str, default: Doc) -> Doc {
    let comments = comments_in(gap);
    if comments.is_empty() {
        return default;
    }
    let mut doc = RcDoc::nil();
    for comment in comments {
        doc = doc.append(RcDoc::hardline()).append(RcDoc::text(comment));
    }
    doc.append(RcDoc::hardline())
}

/// Like [`gap_doc`], but for the gap right before a block's closing
/// bracket/`in` — the one spot where the comment's indentation and the
/// separator's indentation genuinely differ: any comment there is written
/// alongside the block's *contents*, one level deeper than the closing
/// token itself sits (which is why plain `gap_doc` would render it at the
/// wrong, dedented level — `nest` only affects what's inside it, and the
/// trailing separator here is deliberately *outside* the content's `nest`
/// so the closing token lands back at the outer indent).
fn closing_gap_doc(gap: &str, trailing: Doc) -> Doc {
    let comments = comments_in(gap);
    if comments.is_empty() {
        return trailing;
    }
    let mut doc = RcDoc::nil();
    for comment in comments {
        doc = doc.append(RcDoc::hardline()).append(RcDoc::text(comment));
    }
    doc.nest(INDENT).append(trailing)
}

fn format_elem(elem: Elem) -> Piece {
    match elem {
        Elem::Token(t) => Piece {
            doc: RcDoc::text(t.text().to_string()),
            own_line: false,
        },
        Elem::Node(n) => format_node(n),
    }
}

/// The default (pre-comment-override) separator to use before `kinds[i]`,
/// for the "simple" node kinds that just concatenate their children inline
/// with fixed spacing (as opposed to the handful of kinds — collections,
/// `let`/`bind`, `{ }` objects, `switch` — with their own bespoke layout,
/// handled directly in `format_node`).
fn simple_default(kind: SyntaxKind, kinds: &[SyntaxKind], i: usize) -> Doc {
    if matches!(kinds[i], SyntaxKind::COMMA | SyntaxKind::SEMICOLON) {
        return RcDoc::nil();
    }
    match kind {
        SyntaxKind::LIST_EXPR
        | SyntaxKind::TUPLE_EXPR
        | SyntaxKind::MATCHER_TUPLE
        | SyntaxKind::MATCHER_OBJECT
        | SyntaxKind::GROUP_EXPR
        | SyntaxKind::DYNAMIC_ATTR
        | SyntaxKind::ATTR_SEL
        | SyntaxKind::UNARY_EXPR => RcDoc::nil(),
        // `|matchers|`: no space right after the opening pipe, nor right
        // before the closing one — found via `rposition` since it isn't
        // simply "the token right after the last matcher": with zero
        // matchers (`||`) it's also the gap right after the opening pipe.
        SyntaxKind::FUNC_DEF if i == 1 => RcDoc::nil(),
        SyntaxKind::FUNC_DEF if Some(i) == kinds.iter().rposition(|k| *k == SyntaxKind::PIPE) => {
            RcDoc::nil()
        }
        _ => RcDoc::space(),
    }
}

fn format_node(node: SyntaxNode) -> Piece {
    let kind = node.kind();

    if kind == SyntaxKind::STRING_LIT {
        return Piece {
            doc: RcDoc::text(node.text().to_string()),
            own_line: false,
        };
    }

    let (gaps, elems) = real_children(&node);
    let kinds: Vec<SyntaxKind> = elems.iter().map(Elem::kind).collect();
    let pieces: Vec<Piece> = elems.into_iter().map(format_elem).collect();
    let n = pieces.len();

    let doc = match kind {
        // Width-aware: one line if it fits, one item per line (indented)
        // if it doesn't. This is the one place actually using `pretty`'s
        // group/line mechanism instead of a fixed rule — the whole reason
        // for this rewrite was that a fixed "always one line" rule (the
        // previous version of this formatter) doesn't scale to long lists.
        SyntaxKind::LIST_EXPR
        | SyntaxKind::TUPLE_EXPR
        | SyntaxKind::MATCHER_TUPLE
        | SyntaxKind::MATCHER_OBJECT => {
            // `{ }`-style constructs want a space just inside the braces
            // when flat (matching the empty-`OBJECT_EXPR` convention);
            // `[]`/`()`-style ones stay tight against their brackets.
            let edge = if kind == SyntaxKind::MATCHER_OBJECT {
                RcDoc::line()
            } else {
                RcDoc::line_()
            };
            // Empty (just the two bracket tokens, e.g. `[]`/`{}`): `gaps[1]`
            // and `gaps[n - 1]` are the *same* gap here, so applying `edge`
            // at both ends would double it up (visible as `{  }`) — use it
            // exactly once instead.
            if n == 2 {
                return Piece {
                    doc: pieces[0]
                        .doc
                        .clone()
                        .append(gap_doc(&gaps[1], edge))
                        .append(pieces[1].doc.clone())
                        .group(),
                    own_line: false,
                };
            }
            let mut inner = gap_doc(&gaps[1], edge.clone());
            for i in 1..n - 1 {
                inner = inner.append(pieces[i].doc.clone());
                if i + 1 < n - 1 {
                    let sep = if kinds[i + 1] == SyntaxKind::COMMA {
                        RcDoc::nil()
                    } else if kinds[i] == SyntaxKind::COMMA {
                        RcDoc::line()
                    } else {
                        RcDoc::nil()
                    };
                    inner = inner.append(gap_doc(&gaps[i + 1], sep));
                }
            }
            pieces[0]
                .doc
                .clone()
                .append(inner.nest(INDENT))
                .append(closing_gap_doc(&gaps[n - 1], edge))
                .append(pieces[n - 1].doc.clone())
                .group()
        }

        // `(func for [init :] source)` / `[func for source [if filter]]` /
        // `{func for source [if filter]}`: width-aware like the collections
        // above, but the break points are specifically right before `for`
        // and right before `if` (never anywhere else) — each becomes its
        // own line when the whole thing doesn't fit flat. `FOLD_EXPR`
        // always uses `()`, so it stays tight against its parens like a
        // `GROUP_EXPR`; `MAP_EXPR`'s `[]`/`{}` form gets a space just
        // inside instead, matching the hand-written style already used
        // for it elsewhere (`[ x for y ]`, not `[x for y]`).
        SyntaxKind::FOLD_EXPR | SyntaxKind::MAP_EXPR => {
            let edge = if kind == SyntaxKind::FOLD_EXPR {
                RcDoc::line_()
            } else {
                RcDoc::line()
            };
            let mut inner = gap_doc(&gaps[1], edge.clone());
            for i in 1..n - 1 {
                inner = inner.append(pieces[i].doc.clone());
                if i + 1 < n - 1 {
                    let sep = if matches!(kinds[i + 1], SyntaxKind::FOR_KW | SyntaxKind::IF_KW) {
                        RcDoc::line()
                    } else {
                        RcDoc::space()
                    };
                    inner = inner.append(gap_doc(&gaps[i + 1], sep));
                }
            }
            pieces[0]
                .doc
                .clone()
                .append(inner.nest(INDENT))
                .append(closing_gap_doc(&gaps[n - 1], edge))
                .append(pieces[n - 1].doc.clone())
                .group()
        }

        // `{ }` inline when empty, one field per line (always — not
        // width-dependent) otherwise.
        SyntaxKind::OBJECT_EXPR if n <= 2 => pieces[0]
            .doc
            .clone()
            .append(RcDoc::space())
            .append(pieces[n - 1].doc.clone()),
        SyntaxKind::OBJECT_EXPR => {
            let mut inner = RcDoc::nil();
            for i in 1..n - 1 {
                inner = inner
                    .append(gap_doc(&gaps[i], RcDoc::hardline()))
                    .append(pieces[i].doc.clone());
            }
            pieces[0]
                .doc
                .clone()
                .append(inner.nest(INDENT))
                .append(closing_gap_doc(&gaps[n - 1], RcDoc::hardline()))
                .append(pieces[n - 1].doc.clone())
        }

        // `let`/`bind` on their own line, each binding indented on its own
        // line, `in` back at the outer indent, body starting fresh on the
        // line after `in` (also at the outer indent, not indented further).
        SyntaxKind::LET_EXPR | SyntaxKind::BIND_EXPR => {
            let in_idx = kinds
                .iter()
                .position(|k| *k == SyntaxKind::IN_KW)
                .expect("LET_EXPR/BIND_EXPR always has an `in`");
            let mut bindings = RcDoc::nil();
            for i in 1..in_idx {
                bindings = bindings
                    .append(gap_doc(&gaps[i], RcDoc::hardline()))
                    .append(pieces[i].doc.clone());
            }
            pieces[0]
                .doc
                .clone()
                .append(bindings.nest(INDENT))
                .append(closing_gap_doc(&gaps[in_idx], RcDoc::hardline()))
                .append(pieces[in_idx].doc.clone())
                .append(gap_doc(&gaps[in_idx + 1], RcDoc::hardline()))
                .append(pieces[n - 1].doc.clone())
        }

        // `switch source { case* [default] }`: a fresh line (indented)
        // after `{` and after each case, back to the outer indent before
        // `}`. The optional default clause's own tokens (`_`, `=>`, expr,
        // `;`) are flattened directly into this node's children rather
        // than wrapped in their own SWITCH_CASE — they just fall through
        // to the generic per-gap default below like any inline sequence.
        SyntaxKind::SWITCH_EXPR => {
            let lbrace_idx = kinds
                .iter()
                .position(|k| *k == SyntaxKind::L_BRACE)
                .expect("SWITCH_EXPR always has a `{`");
            let mut head = RcDoc::nil();
            for i in 1..=lbrace_idx {
                head = head
                    .append(gap_doc(&gaps[i], RcDoc::space()))
                    .append(pieces[i].doc.clone());
            }
            let mut body = RcDoc::nil();
            for i in (lbrace_idx + 1)..(n - 1) {
                // Every case (and the first one right after `{`) starts a
                // fresh line; anything else here is inside the default
                // clause's own flattened `_ => expr ;` tokens.
                let default =
                    if matches!(kinds[i - 1], SyntaxKind::L_BRACE | SyntaxKind::SWITCH_CASE) {
                        RcDoc::hardline()
                    } else {
                        simple_default(kind, &kinds, i)
                    };
                body = body
                    .append(gap_doc(&gaps[i], default))
                    .append(pieces[i].doc.clone());
            }
            pieces[0]
                .doc
                .clone()
                .append(head)
                .append(body.nest(INDENT))
                .append(closing_gap_doc(&gaps[n - 1], RcDoc::hardline()))
                .append(pieces[n - 1].doc.clone())
        }

        // Every other kind: a flat inline sequence with fixed (not
        // width-dependent) spacing — see `simple_default`. This still
        // covers `let`/`bind` landing as e.g. a func-call argument or list
        // item further down via the `own_line` check below.
        _ => {
            let mut doc = pieces[0].doc.clone();
            for i in 1..n {
                let mut default = simple_default(kind, &kinds, i);
                if pieces[i].own_line {
                    default = RcDoc::hardline();
                }
                doc = doc
                    .append(gap_doc(&gaps[i], default))
                    .append(pieces[i].doc.clone());
            }
            doc
        }
    };

    Piece {
        doc,
        own_line: matches!(kind, SyntaxKind::LET_EXPR | SyntaxKind::BIND_EXPR),
    }
}

pub fn format_source(tree: &SyntaxNode) -> String {
    tree.text().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pblang::parse;

    fn format(source: &str) -> String {
        let tree = parse(source).unwrap_or_else(|e| panic!("failed to parse {source:?}: {e}"));
        format_source(&format_tree(&tree))
    }

    #[test]
    fn let_bindings_get_indented_own_lines() {
        assert_eq!(
            format("let a=1;b=2;in a+b"),
            "let\n    a = 1;\n    b = 2;\nin\na + b"
        );
    }

    #[test]
    fn bind_follows_the_same_layout_as_let() {
        assert_eq!(format("bind a=1;in a"), "bind\n    a = 1;\nin\na");
    }

    #[test]
    fn objects_get_one_field_per_line() {
        assert_eq!(format("{a=1;b=2;}"), "{\n    a = 1;\n    b = 2;\n}");
    }

    #[test]
    fn empty_object_stays_inline() {
        assert_eq!(format("{}"), "{ }");
    }

    #[test]
    fn short_lists_stay_on_a_single_line() {
        assert_eq!(format("[\n  1,\n  2,\n   3\n]"), "[1, 2, 3]");
    }

    #[test]
    fn long_lists_break_one_item_per_line() {
        let items = (0..20)
            .map(|i| format!("\"item number {i}\""))
            .collect::<Vec<_>>()
            .join(",");
        let source = format!("[{items}]");
        let out = format(&source);
        assert!(out.starts_with("[\n    \"item number 0\",\n"), "{out}");
        assert!(out.ends_with("\"item number 19\"\n]"), "{out}");
        for line in out.lines() {
            assert!(line.len() <= WIDTH, "line too long: {line:?}");
        }
    }

    #[test]
    fn func_def_has_no_space_before_the_closing_pipe() {
        assert_eq!(format("|a b|a+b"), "|a b| a + b");
        assert_eq!(format("|a|a"), "|a| a");
    }

    #[test]
    fn object_matchers_get_spaces_just_inside_the_braces() {
        assert_eq!(format("|{a,b,c}|a"), "|{ a, b, c }| a");
        assert_eq!(format("|{a,b,...}|a"), "|{ a, b, ... }| a");
    }

    #[test]
    fn empty_brackets_stay_tight_with_no_doubled_up_separator() {
        // A regression check: the empty case shares one gap between the
        // "after open bracket" and "before close bracket" edges, so it's
        // easy to accidentally apply the separator twice (`{  }`) — or, for
        // list/tuple/matcher-tuple's `line_()`, to forget the `group()`
        // that lets it collapse to nothing at all instead of always
        // breaking.
        assert_eq!(format("[]"), "[]");
        assert_eq!(format("|{}|1"), "|{ }| 1");
    }

    #[test]
    fn a_comment_right_before_a_closing_brace_matches_its_siblings_indent() {
        // Regression: the gap right before `}` is deliberately *outside*
        // the `nest()` that indents the object's fields (so `}` itself
        // dedents back), but a comment sitting in that same gap belongs
        // with the fields, not with `}` — it needs its own `nest()`.
        assert_eq!(
            format("{a=1;\n# trailing\n}"),
            "{\n    a = 1;\n    # trailing\n}"
        );
    }

    #[test]
    fn switch_puts_every_case_on_its_own_line_including_the_first() {
        // Regression: only cases *after* the first were getting a forced
        // line break — the first one (right after `{`) was falling through
        // to the generic space-separated default.
        assert_eq!(
            format("switch x{1=>10;2=>20;}"),
            "switch x {\n    1 => 10;\n    2 => 20;\n}"
        );
    }

    #[test]
    fn long_maps_and_folds_break_before_for_and_if() {
        let source = "[|item| item.a_rather_long_field_name_here for a_pretty_long_source_name_here_too if |x| x.is_enabled_flag]";
        assert_eq!(
            format(source),
            "[\n    |item| item.a_rather_long_field_name_here\n    for a_pretty_long_source_name_here_too\n    if |x| x.is_enabled_flag\n]"
        );
    }

    #[test]
    fn a_let_nested_inside_an_object_value_indents_one_level_deeper() {
        // `let` always starts its own line, even right after `=` — same
        // rule that puts it on its own line after a `|matcher|` func def.
        assert_eq!(
            format("{x=let a=1;in a;}"),
            "{\n    x =\n    let\n        a = 1;\n    in\n    a;\n}"
        );
    }

    #[test]
    fn a_let_inside_a_list_breaks_the_list_too() {
        // `let`/`in` can never render on one line by construction (that's
        // the whole point of the "own line" rule), so a list containing
        // one can't stay flat either — `group()` sees the embedded
        // `hardline` and (correctly) breaks every item onto its own line,
        // same as it would for any other reason the flat form doesn't fit.
        assert_eq!(
            format("[let a=1;in a, 2]"),
            "[\n    let\n        a = 1;\n    in\n    a,\n    2\n]"
        );
    }

    #[test]
    fn comments_are_preserved_on_their_own_line() {
        assert_eq!(
            format("let\n# hello\na=1;\nin a"),
            "let\n    # hello\n    a = 1;\nin\na"
        );
    }

    #[test]
    fn string_literals_are_copied_through_byte_for_byte() {
        // Odd internal spacing inside the interpolation and the raw chunk
        // text must survive untouched.
        let src = r#"let x=1;in "a ${  x  } b""#;
        assert_eq!(format(src), "let\n    x = 1;\nin\n\"a ${  x  } b\"");
    }

    #[test]
    fn format_tree_is_idempotent() {
        let source = "let a=1;b={x=2;y=[1,2,3];};in a+b.x";
        let once = format(source);
        let twice = format(&once);
        assert_eq!(once, twice);
    }
}
