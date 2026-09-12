//! A shared, grammar-aware walk over the `pblang` parse tree.
//!
//! [`LangVisitor`] is the one place that knows how each `SyntaxKind` node is
//! shaped (which child is the body, which children are a list of bindings,
//! how a `switch` case splits into pattern/result, ...). [`walk_expr`] and
//! [`walk_matcher`] do that extraction, and hand each child to the matching
//! `LangVisitor` method as an [`UnvisitedExpr`]/[`UnvisitedMatcher`] handle
//! rather than an already-visited value — an implementor never touches
//! `SyntaxNode::children()` itself, but it does decide which handles to call
//! `.visit()` on (skipping ones it won't use) and what [`LangVisitor::Down`]
//! state to pass each. This lets `pbexpr` (lowering the tree into its own
//! `Expr`/`Matcher` runtime tree) and `pbls` (deriving lighter-weight
//! semantic views for LSP features) share one traversal instead of each
//! re-deriving the grammar's shape.

use super::syntaxtree::{SyntaxKind, SyntaxNode, SyntaxToken};
use rowan::{NodeOrToken, TextRange};

/// An `ASSIGNMENT`'s key: either a bare `IDENT` token, or a `STRING_LIT`
/// node (handed over raw, since resolving it — unescaping, rejecting
/// interpolation — is caller-specific policy, not grammar shape).
pub enum AssignKey {
    Ident(SyntaxToken),
    StringLit(SyntaxNode),
}

/// An `ASSIGNMENT`'s value: either an explicit `<expr>` after `=`, or
/// absent entirely — the `OBJECT_EXPR`-only shorthand `<key> ;` (no `EQ`
/// token, no value node), meaning "value is a reference to a variable
/// named `<key>`." `BIND_EXPR`'s `ASSIGNMENT`s always have an explicit
/// value; only `OBJECT_EXPR`'s grammar production allows the shorthand.
pub enum AssignValue {
    Expr(UnvisitedExpr),
    Shorthand,
}

/// The right-hand side of an `ATTR_SEL`'s `.`: either a bare `IDENT` token
/// (`.foo`) or an already visited dynamic expression (`.{foo}`).
pub enum AttrSelector<E> {
    Static(SyntaxToken),
    Dynamic(E),
}

/// One piece of a `STRING_LIT`: a raw literal chunk, or an already visited
/// `${..}` interpolation.
pub enum StringPart<E> {
    Chunk(SyntaxToken),
    Embed(E),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKind {
    List,
    Object,
}

/// One `OBJECT_MATCHER_FIELD`: `<ident> [= <matcher>] [? <expr>]`. `matcher`
/// is `None` for the shorthand form (`{foo}`, meaning "bind to a variable
/// named `foo`") — deciding what that shorthand means is caller policy, not
/// grammar shape, so it isn't defaulted here.
pub struct ObjectField<M, E> {
    pub key: SyntaxToken,
    pub matcher: Option<M>,
    pub default: Option<E>,
}

/// A not-yet-walked expression child, handed to a `LangVisitor` method
/// instead of an already-visited value. `walk_expr` builds these; a visitor
/// calls [`UnvisitedExpr::visit`] on the ones it actually wants, passing
/// whatever `Down` state applies to that child, and simply drops the rest —
/// no work is done for a handle that's never visited.
pub struct UnvisitedExpr(SyntaxNode);

impl UnvisitedExpr {
    /// The child's own span, available without visiting it.
    pub fn range(&self) -> TextRange {
        self.0.text_range()
    }

    pub fn visit<V: LangVisitor>(self, v: &mut V, down: &V::Down) -> Result<V::Expr, V::Error> {
        walk_expr(&self.0, v, down)
    }
}

/// The matcher counterpart of [`UnvisitedExpr`].
pub struct UnvisitedMatcher(SyntaxNode);

impl UnvisitedMatcher {
    /// The child's own span, available without visiting it.
    pub fn range(&self) -> TextRange {
        self.0.text_range()
    }

    pub fn visit<V: LangVisitor>(self, v: &mut V, down: &V::Down) -> Result<V::Matcher, V::Error> {
        walk_matcher(&self.0, v, down)
    }
}

/// Visits every item in order with the same `down` state — the common case
/// for a visitor that always wants every child, so it doesn't need to spell
/// out the loop itself.
pub fn visit_all<V: LangVisitor>(
    items: impl IntoIterator<Item = UnvisitedExpr>,
    v: &mut V,
    down: &V::Down,
) -> Result<Vec<V::Expr>, V::Error> {
    items.into_iter().map(|u| u.visit(v, down)).collect()
}

/// The matcher counterpart of [`visit_all`].
pub fn visit_all_matchers<V: LangVisitor>(
    items: impl IntoIterator<Item = UnvisitedMatcher>,
    v: &mut V,
    down: &V::Down,
) -> Result<Vec<V::Matcher>, V::Error> {
    items.into_iter().map(|u| u.visit(v, down)).collect()
}

/// Implemented once per consumer of the parse tree. `Expr`/`Matcher` are
/// whatever that consumer wants to produce per expression/matcher node
/// (e.g. `pbexpr`'s own runtime `Expr<T,F>`/`Matcher<T,F>`, or `()` for a
/// side-effecting LSP walk). `Down` is state threaded top-down through the
/// walk (e.g. a scope environment); a visitor that doesn't need one uses
/// `type Down = ();`. There's no `visit_group` — `GROUP_EXPR` (`( <expr> )`)
/// is transparent: `walk_expr` recurses straight through it without
/// producing a node of its own, matching what parenthesization means.
pub trait LangVisitor {
    type Expr;
    type Matcher;
    type Error;
    type Down;

    /// Each binding's own `range` is the whole `<matcher> = <expr> ;`
    /// statement (a `LET_BINDING` node) — distinct from `Self::Matcher`,
    /// which is whatever the matcher itself was folded into.
    fn visit_let(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        bindings: Vec<(TextRange, UnvisitedMatcher, UnvisitedExpr)>,
        body: UnvisitedExpr,
    ) -> Result<Self::Expr, Self::Error>;
    /// Each item's own `range` is the whole `<key> = <expr> ;` statement (an
    /// `ASSIGNMENT` node). `BIND_EXPR`'s grammar production never allows the
    /// shorthand form, so `value` is always `AssignValue::Expr` here.
    fn visit_bind(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        items: Vec<(TextRange, AssignKey, AssignValue)>,
        body: UnvisitedExpr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_func_def(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        params: Vec<UnvisitedMatcher>,
        body: UnvisitedExpr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_binary(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        op: SyntaxToken,
        lhs: UnvisitedExpr,
        rhs: UnvisitedExpr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_unary(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        op: SyntaxToken,
        operand: UnvisitedExpr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_func_call(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        func: UnvisitedExpr,
        arg: UnvisitedExpr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_attr_sel(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        base: UnvisitedExpr,
        attr: AttrSelector<UnvisitedExpr>,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_fold(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        func: UnvisitedExpr,
        init: Option<UnvisitedExpr>,
        input: UnvisitedExpr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_map(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        kind: MapKind,
        func: UnvisitedExpr,
        input: UnvisitedExpr,
        filter: Option<UnvisitedExpr>,
    ) -> Result<Self::Expr, Self::Error>;
    /// `cases`' pattern side is `Self::Expr`, not `Self::Matcher`: despite
    /// `SwitchCase`'s grammar doc calling it a matcher, `SWITCH_CASE`'s
    /// first child is grammatically an expression (switch compares by
    /// value, not by structural pattern match).
    fn visit_switch(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        input: UnvisitedExpr,
        cases: Vec<(UnvisitedExpr, UnvisitedExpr)>,
        default: Option<UnvisitedExpr>,
    ) -> Result<Self::Expr, Self::Error>;
    /// Each item's own `range` is the whole `<key> = <expr> ;` (or bare
    /// `<key> ;` shorthand) statement (an `ASSIGNMENT` node).
    fn visit_object(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        items: Vec<(TextRange, AssignKey, AssignValue)>,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_list(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        items: Vec<UnvisitedExpr>,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_tuple(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        items: Vec<UnvisitedExpr>,
    ) -> Result<Self::Expr, Self::Error>;
    /// `token` is `TRUE_KW`/`FALSE_KW`/`NUMBER`/`NULL_KW`.
    fn visit_literal(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        token: SyntaxToken,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_string(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        parts: Vec<StringPart<UnvisitedExpr>>,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_var(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        name: SyntaxToken,
    ) -> Result<Self::Expr, Self::Error>;

    /// `range` is the whole matcher node's own span — kept on every matcher
    /// method (mirroring the expression methods) even though `pbexpr`'s own
    /// `Matcher<T,F>` doesn't store one, since it's the only way a
    /// position-aware consumer (e.g. an LSP outline) can recover "where did
    /// this pattern's text come from" for compound matchers.
    fn visit_matcher_ident(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        name: SyntaxToken,
    ) -> Result<Self::Matcher, Self::Error>;
    fn visit_matcher_wildcard(
        &mut self,
        range: TextRange,
        down: &Self::Down,
    ) -> Result<Self::Matcher, Self::Error>;
    fn visit_matcher_alias(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        inner: UnvisitedMatcher,
        name: SyntaxToken,
    ) -> Result<Self::Matcher, Self::Error>;
    fn visit_matcher_tuple(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        items: Vec<UnvisitedMatcher>,
    ) -> Result<Self::Matcher, Self::Error>;
    fn visit_matcher_object(
        &mut self,
        range: TextRange,
        down: &Self::Down,
        exhaustive: bool,
        fields: Vec<ObjectField<UnvisitedMatcher, UnvisitedExpr>>,
    ) -> Result<Self::Matcher, Self::Error>;
}

/// Direct, non-trivia token children of `node`, in source order. `TRIVIA` is
/// the synthetic space-padding `green_node` inserts for skipped
/// whitespace/comments; it never carries meaning.
fn non_trivia_tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + '_ {
    node.children_with_tokens()
        .filter_map(|el| el.into_token())
        .filter(|t| t.kind() != SyntaxKind::TRIVIA)
}

fn first_non_trivia_token(node: &SyntaxNode) -> SyntaxToken {
    non_trivia_tokens(node)
        .next()
        .unwrap_or_else(|| panic!("{:?} has at least one real token", node.kind()))
}

fn find_token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    non_trivia_tokens(node).find(|t| t.kind() == kind)
}

fn has_token(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    find_token(node, kind).is_some()
}

/// The single non-trivia operator token of a `BINARY_EXPR`/`UNARY_EXPR`
/// (the one thing between/before its expression child/children).
fn operator_token(node: &SyntaxNode) -> SyntaxToken {
    non_trivia_tokens(node)
        .next()
        .unwrap_or_else(|| panic!("{:?} has an operator token", node.kind()))
}

fn extract_assignment(assignment: &SyntaxNode) -> (TextRange, AssignKey, AssignValue) {
    let range = assignment.text_range();
    if let Some(ident) = find_token(assignment, SyntaxKind::IDENT) {
        // Node children exclude tokens, so the shorthand `<key> ;` form
        // (only IDENT/SEMICOLON tokens, no value node) naturally yields
        // `None` here.
        let value = match assignment.children().next() {
            Some(value_node) => AssignValue::Expr(UnvisitedExpr(value_node)),
            None => AssignValue::Shorthand,
        };
        return (range, AssignKey::Ident(ident), value);
    }
    let mut children = assignment.children();
    let key_node = children.next().expect("ASSIGNMENT key is a STRING_LIT");
    let value_node = children.next().expect("ASSIGNMENT always has a value");
    (
        range,
        AssignKey::StringLit(key_node),
        AssignValue::Expr(UnvisitedExpr(value_node)),
    )
}

fn extract_object_matcher_field(
    field: &SyntaxNode,
) -> ObjectField<UnvisitedMatcher, UnvisitedExpr> {
    let key = first_non_trivia_token(field);
    let has_eq = has_token(field, SyntaxKind::EQ);
    let has_default = has_token(field, SyntaxKind::QUESTION);
    let children: Vec<SyntaxNode> = field.children().collect();
    let (matcher, default) = match (has_eq, has_default) {
        (false, false) => (None, None),
        (true, false) => (Some(UnvisitedMatcher(children[0].clone())), None),
        (false, true) => (None, Some(UnvisitedExpr(children[0].clone()))),
        (true, true) => (
            Some(UnvisitedMatcher(children[0].clone())),
            Some(UnvisitedExpr(children[1].clone())),
        ),
    };
    ObjectField {
        key,
        matcher,
        default,
    }
}

/// Walks one expression node, extracting its children and handing each one
/// to the matching [`LangVisitor`] method as an [`UnvisitedExpr`]/
/// [`UnvisitedMatcher`] handle — recursion only happens for the handles the
/// visitor actually calls `.visit()` on.
pub fn walk_expr<V: LangVisitor>(
    node: &SyntaxNode,
    v: &mut V,
    down: &V::Down,
) -> Result<V::Expr, V::Error> {
    let range = node.text_range();
    let children: Vec<SyntaxNode> = node.children().collect();

    match node.kind() {
        SyntaxKind::GROUP_EXPR => walk_expr(&children[0], v, down),

        SyntaxKind::LET_EXPR => {
            let body_node = children.last().expect("LET_EXPR has a body").clone();
            let binding_nodes = &children[..children.len() - 1];
            let mut bindings = Vec::with_capacity(binding_nodes.len());
            for binding in binding_nodes {
                let bc: Vec<SyntaxNode> = binding.children().collect();
                bindings.push((
                    binding.text_range(),
                    UnvisitedMatcher(bc[0].clone()),
                    UnvisitedExpr(bc[1].clone()),
                ));
            }
            v.visit_let(range, down, bindings, UnvisitedExpr(body_node))
        }

        SyntaxKind::BIND_EXPR => {
            let body_node = children.last().expect("BIND_EXPR has a body").clone();
            let assign_nodes = &children[..children.len() - 1];
            let items = assign_nodes.iter().map(extract_assignment).collect();
            v.visit_bind(range, down, items, UnvisitedExpr(body_node))
        }

        SyntaxKind::FUNC_DEF => {
            let body_node = children.last().expect("FUNC_DEF has a body").clone();
            let matcher_nodes = &children[..children.len() - 1];
            let params = matcher_nodes
                .iter()
                .cloned()
                .map(UnvisitedMatcher)
                .collect();
            v.visit_func_def(range, down, params, UnvisitedExpr(body_node))
        }

        SyntaxKind::BINARY_EXPR => {
            let op = operator_token(node);
            v.visit_binary(
                range,
                down,
                op,
                UnvisitedExpr(children[0].clone()),
                UnvisitedExpr(children[1].clone()),
            )
        }

        SyntaxKind::UNARY_EXPR => {
            let op = operator_token(node);
            v.visit_unary(range, down, op, UnvisitedExpr(children[0].clone()))
        }

        SyntaxKind::FUNC_CALL => v.visit_func_call(
            range,
            down,
            UnvisitedExpr(children[0].clone()),
            UnvisitedExpr(children[1].clone()),
        ),

        SyntaxKind::ATTR_SEL => {
            let base = UnvisitedExpr(children[0].clone());
            let attr = if children.len() == 2 {
                let inner = children[1]
                    .children()
                    .next()
                    .expect("DYNAMIC_ATTR wraps an expr");
                AttrSelector::Dynamic(UnvisitedExpr(inner))
            } else {
                let name = find_token(node, SyntaxKind::IDENT)
                    .expect("static ATTR_SEL has an IDENT token");
                AttrSelector::Static(name)
            };
            v.visit_attr_sel(range, down, base, attr)
        }

        SyntaxKind::FOLD_EXPR => {
            let has_init = has_token(node, SyntaxKind::COLON);
            let (func_node, init_node, input_node) = if has_init {
                (&children[0], Some(&children[1]), &children[2])
            } else {
                (&children[0], None, &children[1])
            };
            let func = UnvisitedExpr(func_node.clone());
            let init = init_node.map(|n| UnvisitedExpr(n.clone()));
            let input = UnvisitedExpr(input_node.clone());
            v.visit_fold(range, down, func, init, input)
        }

        SyntaxKind::MAP_EXPR => {
            let kind = match first_non_trivia_token(node).kind() {
                SyntaxKind::L_BRACKET => MapKind::List,
                SyntaxKind::L_BRACE => MapKind::Object,
                other => unreachable!("MAP_EXPR starts with unexpected token {other:?}"),
            };
            let has_filter = has_token(node, SyntaxKind::IF_KW);
            let (func_node, input_node, filter_node) = if has_filter {
                (&children[0], &children[1], Some(&children[2]))
            } else {
                (&children[0], &children[1], None)
            };
            let func = UnvisitedExpr(func_node.clone());
            let input = UnvisitedExpr(input_node.clone());
            let filter = filter_node.map(|n| UnvisitedExpr(n.clone()));
            v.visit_map(range, down, kind, func, input, filter)
        }

        SyntaxKind::SWITCH_EXPR => {
            let mut iter = children.iter();
            let input_node = iter.next().expect("SWITCH_EXPR has an input").clone();
            let mut cases = Vec::new();
            let mut default = None;
            for child in iter {
                if child.kind() == SyntaxKind::SWITCH_CASE {
                    let cc: Vec<SyntaxNode> = child.children().collect();
                    cases.push((UnvisitedExpr(cc[0].clone()), UnvisitedExpr(cc[1].clone())));
                } else {
                    default = Some(UnvisitedExpr(child.clone()));
                }
            }
            v.visit_switch(range, down, UnvisitedExpr(input_node), cases, default)
        }

        SyntaxKind::OBJECT_EXPR => {
            let items = children.iter().map(extract_assignment).collect();
            v.visit_object(range, down, items)
        }

        SyntaxKind::LIST_EXPR => {
            let items = children.into_iter().map(UnvisitedExpr).collect();
            v.visit_list(range, down, items)
        }

        SyntaxKind::TUPLE_EXPR => {
            let items = children.into_iter().map(UnvisitedExpr).collect();
            v.visit_tuple(range, down, items)
        }

        SyntaxKind::LITERAL_EXPR => {
            let token = first_non_trivia_token(node);
            v.visit_literal(range, down, token)
        }

        SyntaxKind::STRING_LIT => {
            let mut parts = Vec::new();
            for el in node.children_with_tokens() {
                match el {
                    NodeOrToken::Token(t) if t.kind() == SyntaxKind::STRING_CHUNK => {
                        parts.push(StringPart::Chunk(t));
                    }
                    NodeOrToken::Node(n) if n.kind() == SyntaxKind::STRING_EMBED => {
                        let inner = n.children().next().expect("STRING_EMBED wraps an expr");
                        parts.push(StringPart::Embed(UnvisitedExpr(inner)));
                    }
                    _ => {}
                }
            }
            v.visit_string(range, down, parts)
        }

        SyntaxKind::VAR_EXPR => {
            let name = first_non_trivia_token(node);
            v.visit_var(range, down, name)
        }

        other => unreachable!("unexpected expression node kind {other:?}"),
    }
}

/// Walks one matcher node, extracting its children and handing each one to
/// the matching [`LangVisitor`] method as an [`UnvisitedMatcher`]/
/// [`UnvisitedExpr`] handle — see [`walk_expr`].
pub fn walk_matcher<V: LangVisitor>(
    node: &SyntaxNode,
    v: &mut V,
    down: &V::Down,
) -> Result<V::Matcher, V::Error> {
    let range = node.text_range();
    let children: Vec<SyntaxNode> = node.children().collect();

    match node.kind() {
        SyntaxKind::MATCHER_IDENT => {
            v.visit_matcher_ident(range, down, first_non_trivia_token(node))
        }

        SyntaxKind::MATCHER_WILDCARD => v.visit_matcher_wildcard(range, down),

        SyntaxKind::MATCHER_ALIAS => {
            let inner = UnvisitedMatcher(children[0].clone());
            let name = find_token(node, SyntaxKind::IDENT).expect("MATCHER_ALIAS has a name");
            v.visit_matcher_alias(range, down, inner, name)
        }

        SyntaxKind::MATCHER_TUPLE => {
            let items = children.into_iter().map(UnvisitedMatcher).collect();
            v.visit_matcher_tuple(range, down, items)
        }

        SyntaxKind::MATCHER_OBJECT => {
            let exhaustive = !has_token(node, SyntaxKind::DOT_DOT_DOT);
            let fields = children.iter().map(extract_object_matcher_field).collect();
            v.visit_matcher_object(range, down, exhaustive, fields)
        }

        other => unreachable!("unexpected matcher node kind {other:?}"),
    }
}
