//! A shared, grammar-aware walk over the `pblang` parse tree.
//!
//! [`LangVisitor`] is the one place that knows how each `SyntaxKind` node is
//! shaped (which child is the body, which children are a list of bindings,
//! how a `switch` case splits into pattern/result, ...). [`walk_expr`] and
//! [`walk_matcher`] do that extraction and the recursion, and hand already
//! extracted, already visited parts to the matching `LangVisitor` method —
//! an implementor never touches `SyntaxNode::children()` itself. This lets
//! `pbexpr` (lowering the tree into its own `Expr`/`Matcher` runtime tree)
//! and `pbls` (deriving lighter-weight semantic views for LSP features)
//! share one traversal instead of each re-deriving the grammar's shape.

use super::syntaxtree::{SyntaxKind, SyntaxNode, SyntaxToken};
use rowan::{NodeOrToken, TextRange};

/// An `ASSIGNMENT`'s key: either a bare `IDENT` token, or a `STRING_LIT`
/// node (handed over raw, since resolving it — unescaping, rejecting
/// interpolation — is caller-specific policy, not grammar shape).
pub enum AssignKey {
    Ident(SyntaxToken),
    StringLit(SyntaxNode),
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

/// Implemented once per consumer of the parse tree. `Expr`/`Matcher` are
/// whatever that consumer wants to produce per expression/matcher node
/// (e.g. `pbexpr`'s own runtime `Expr<T,F>`/`Matcher<T,F>`, or `()` for a
/// side-effecting LSP walk). There's no `visit_group` — `GROUP_EXPR`
/// (`( <expr> )`) is transparent: `walk_expr` recurses straight through it
/// without producing a node of its own, matching what parenthesization
/// means.
pub trait LangVisitor {
    type Expr;
    type Matcher;
    type Error;

    /// Each binding's own `range` is the whole `<matcher> = <expr> ;`
    /// statement (a `LET_BINDING` node) — distinct from `Self::Matcher`,
    /// which is whatever the matcher itself was folded into.
    fn visit_let(
        &mut self,
        range: TextRange,
        bindings: Vec<(TextRange, Self::Matcher, Self::Expr)>,
        body: Self::Expr,
    ) -> Result<Self::Expr, Self::Error>;
    /// Each item's own `range` is the whole `<key> = <expr> ;` statement (an
    /// `ASSIGNMENT` node).
    fn visit_bind(
        &mut self,
        range: TextRange,
        items: Vec<(TextRange, AssignKey, Self::Expr)>,
        body: Self::Expr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_func_def(
        &mut self,
        range: TextRange,
        params: Vec<Self::Matcher>,
        body: Self::Expr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_binary(
        &mut self,
        range: TextRange,
        op: SyntaxToken,
        lhs: Self::Expr,
        rhs: Self::Expr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_unary(
        &mut self,
        range: TextRange,
        op: SyntaxToken,
        operand: Self::Expr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_func_call(
        &mut self,
        range: TextRange,
        func: Self::Expr,
        arg: Self::Expr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_attr_sel(
        &mut self,
        range: TextRange,
        base: Self::Expr,
        attr: AttrSelector<Self::Expr>,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_fold(
        &mut self,
        range: TextRange,
        func: Self::Expr,
        init: Option<Self::Expr>,
        input: Self::Expr,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_map(
        &mut self,
        range: TextRange,
        kind: MapKind,
        func: Self::Expr,
        input: Self::Expr,
        filter: Option<Self::Expr>,
    ) -> Result<Self::Expr, Self::Error>;
    /// `cases`' pattern side is `Self::Expr`, not `Self::Matcher`: despite
    /// `SwitchCase`'s grammar doc calling it a matcher, `SWITCH_CASE`'s
    /// first child is grammatically an expression (switch compares by
    /// value, not by structural pattern match).
    fn visit_switch(
        &mut self,
        range: TextRange,
        input: Self::Expr,
        cases: Vec<(Self::Expr, Self::Expr)>,
        default: Option<Self::Expr>,
    ) -> Result<Self::Expr, Self::Error>;
    /// Each item's own `range` is the whole `<key> = <expr> ;` statement (an
    /// `ASSIGNMENT` node).
    fn visit_object(
        &mut self,
        range: TextRange,
        items: Vec<(TextRange, AssignKey, Self::Expr)>,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_list(
        &mut self,
        range: TextRange,
        items: Vec<Self::Expr>,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_tuple(
        &mut self,
        range: TextRange,
        items: Vec<Self::Expr>,
    ) -> Result<Self::Expr, Self::Error>;
    /// `token` is `TRUE_KW`/`FALSE_KW`/`NUMBER`/`NULL_KW`.
    fn visit_literal(
        &mut self,
        range: TextRange,
        token: SyntaxToken,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_string(
        &mut self,
        range: TextRange,
        parts: Vec<StringPart<Self::Expr>>,
    ) -> Result<Self::Expr, Self::Error>;
    fn visit_var(&mut self, range: TextRange, name: SyntaxToken)
    -> Result<Self::Expr, Self::Error>;

    /// `range` is the whole matcher node's own span — kept on every matcher
    /// method (mirroring the expression methods) even though `pbexpr`'s own
    /// `Matcher<T,F>` doesn't store one, since it's the only way a
    /// position-aware consumer (e.g. an LSP outline) can recover "where did
    /// this pattern's text come from" for compound matchers.
    fn visit_matcher_ident(
        &mut self,
        range: TextRange,
        name: SyntaxToken,
    ) -> Result<Self::Matcher, Self::Error>;
    fn visit_matcher_wildcard(&mut self, range: TextRange) -> Result<Self::Matcher, Self::Error>;
    fn visit_matcher_alias(
        &mut self,
        range: TextRange,
        inner: Self::Matcher,
        name: SyntaxToken,
    ) -> Result<Self::Matcher, Self::Error>;
    fn visit_matcher_tuple(
        &mut self,
        range: TextRange,
        items: Vec<Self::Matcher>,
    ) -> Result<Self::Matcher, Self::Error>;
    fn visit_matcher_object(
        &mut self,
        range: TextRange,
        exhaustive: bool,
        fields: Vec<ObjectField<Self::Matcher, Self::Expr>>,
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

fn extract_assignment<V: LangVisitor>(
    assignment: &SyntaxNode,
    v: &mut V,
) -> Result<(TextRange, AssignKey, V::Expr), V::Error> {
    let range = assignment.text_range();
    if let Some(ident) = find_token(assignment, SyntaxKind::IDENT) {
        let value_node = assignment
            .children()
            .next()
            .expect("ASSIGNMENT always has a value");
        let value = walk_expr(&value_node, v)?;
        return Ok((range, AssignKey::Ident(ident), value));
    }
    let mut children = assignment.children();
    let key_node = children.next().expect("ASSIGNMENT key is a STRING_LIT");
    let value_node = children.next().expect("ASSIGNMENT always has a value");
    let value = walk_expr(&value_node, v)?;
    Ok((range, AssignKey::StringLit(key_node), value))
}

fn extract_object_matcher_field<V: LangVisitor>(
    field: &SyntaxNode,
    v: &mut V,
) -> Result<ObjectField<V::Matcher, V::Expr>, V::Error> {
    let key = first_non_trivia_token(field);
    let has_eq = has_token(field, SyntaxKind::EQ);
    let has_default = has_token(field, SyntaxKind::QUESTION);
    let children: Vec<SyntaxNode> = field.children().collect();
    let (matcher, default) = match (has_eq, has_default) {
        (false, false) => (None, None),
        (true, false) => (Some(walk_matcher(&children[0], v)?), None),
        (false, true) => (None, Some(walk_expr(&children[0], v)?)),
        (true, true) => (
            Some(walk_matcher(&children[0], v)?),
            Some(walk_expr(&children[1], v)?),
        ),
    };
    Ok(ObjectField {
        key,
        matcher,
        default,
    })
}

/// Walks one expression node, extracting its children and recursing before
/// handing the result to the matching [`LangVisitor`] method.
pub fn walk_expr<V: LangVisitor>(node: &SyntaxNode, v: &mut V) -> Result<V::Expr, V::Error> {
    let range = node.text_range();
    let children: Vec<SyntaxNode> = node.children().collect();

    match node.kind() {
        SyntaxKind::GROUP_EXPR => walk_expr(&children[0], v),

        SyntaxKind::LET_EXPR => {
            let body_node = children.last().expect("LET_EXPR has a body").clone();
            let binding_nodes = &children[..children.len() - 1];
            let mut bindings = Vec::with_capacity(binding_nodes.len());
            for binding in binding_nodes {
                let bc: Vec<SyntaxNode> = binding.children().collect();
                let matcher = walk_matcher(&bc[0], v)?;
                let value = walk_expr(&bc[1], v)?;
                bindings.push((binding.text_range(), matcher, value));
            }
            let body = walk_expr(&body_node, v)?;
            v.visit_let(range, bindings, body)
        }

        SyntaxKind::BIND_EXPR => {
            let body_node = children.last().expect("BIND_EXPR has a body").clone();
            let assign_nodes = &children[..children.len() - 1];
            let mut items = Vec::with_capacity(assign_nodes.len());
            for assignment in assign_nodes {
                items.push(extract_assignment(assignment, v)?);
            }
            let body = walk_expr(&body_node, v)?;
            v.visit_bind(range, items, body)
        }

        SyntaxKind::FUNC_DEF => {
            let body_node = children.last().expect("FUNC_DEF has a body").clone();
            let matcher_nodes = &children[..children.len() - 1];
            let mut params = Vec::with_capacity(matcher_nodes.len());
            for matcher in matcher_nodes {
                params.push(walk_matcher(matcher, v)?);
            }
            let body = walk_expr(&body_node, v)?;
            v.visit_func_def(range, params, body)
        }

        SyntaxKind::BINARY_EXPR => {
            let op = operator_token(node);
            let lhs = walk_expr(&children[0], v)?;
            let rhs = walk_expr(&children[1], v)?;
            v.visit_binary(range, op, lhs, rhs)
        }

        SyntaxKind::UNARY_EXPR => {
            let op = operator_token(node);
            let operand = walk_expr(&children[0], v)?;
            v.visit_unary(range, op, operand)
        }

        SyntaxKind::FUNC_CALL => {
            let func = walk_expr(&children[0], v)?;
            let arg = walk_expr(&children[1], v)?;
            v.visit_func_call(range, func, arg)
        }

        SyntaxKind::ATTR_SEL => {
            let base = walk_expr(&children[0], v)?;
            let attr = if children.len() == 2 {
                let inner = children[1]
                    .children()
                    .next()
                    .expect("DYNAMIC_ATTR wraps an expr");
                AttrSelector::Dynamic(walk_expr(&inner, v)?)
            } else {
                let name = find_token(node, SyntaxKind::IDENT)
                    .expect("static ATTR_SEL has an IDENT token");
                AttrSelector::Static(name)
            };
            v.visit_attr_sel(range, base, attr)
        }

        SyntaxKind::FOLD_EXPR => {
            let has_init = has_token(node, SyntaxKind::COLON);
            let (func_node, init_node, input_node) = if has_init {
                (&children[0], Some(&children[1]), &children[2])
            } else {
                (&children[0], None, &children[1])
            };
            let func = walk_expr(func_node, v)?;
            let init = match init_node {
                Some(n) => Some(walk_expr(n, v)?),
                None => None,
            };
            let input = walk_expr(input_node, v)?;
            v.visit_fold(range, func, init, input)
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
            let func = walk_expr(func_node, v)?;
            let input = walk_expr(input_node, v)?;
            let filter = match filter_node {
                Some(n) => Some(walk_expr(n, v)?),
                None => None,
            };
            v.visit_map(range, kind, func, input, filter)
        }

        SyntaxKind::SWITCH_EXPR => {
            let mut iter = children.iter();
            let input_node = iter.next().expect("SWITCH_EXPR has an input");
            let mut cases = Vec::new();
            let mut default = None;
            for child in iter {
                if child.kind() == SyntaxKind::SWITCH_CASE {
                    let cc: Vec<SyntaxNode> = child.children().collect();
                    let pattern = walk_expr(&cc[0], v)?;
                    let result = walk_expr(&cc[1], v)?;
                    cases.push((pattern, result));
                } else {
                    default = Some(walk_expr(child, v)?);
                }
            }
            let input = walk_expr(input_node, v)?;
            v.visit_switch(range, input, cases, default)
        }

        SyntaxKind::OBJECT_EXPR => {
            let mut items = Vec::with_capacity(children.len());
            for assignment in &children {
                items.push(extract_assignment(assignment, v)?);
            }
            v.visit_object(range, items)
        }

        SyntaxKind::LIST_EXPR => {
            let mut items = Vec::with_capacity(children.len());
            for item in &children {
                items.push(walk_expr(item, v)?);
            }
            v.visit_list(range, items)
        }

        SyntaxKind::TUPLE_EXPR => {
            let mut items = Vec::with_capacity(children.len());
            for item in &children {
                items.push(walk_expr(item, v)?);
            }
            v.visit_tuple(range, items)
        }

        SyntaxKind::LITERAL_EXPR => {
            let token = first_non_trivia_token(node);
            v.visit_literal(range, token)
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
                        parts.push(StringPart::Embed(walk_expr(&inner, v)?));
                    }
                    _ => {}
                }
            }
            v.visit_string(range, parts)
        }

        SyntaxKind::VAR_EXPR => {
            let name = first_non_trivia_token(node);
            v.visit_var(range, name)
        }

        other => unreachable!("unexpected expression node kind {other:?}"),
    }
}

/// Walks one matcher node, extracting its children and recursing before
/// handing the result to the matching [`LangVisitor`] method.
pub fn walk_matcher<V: LangVisitor>(node: &SyntaxNode, v: &mut V) -> Result<V::Matcher, V::Error> {
    let range = node.text_range();
    let children: Vec<SyntaxNode> = node.children().collect();

    match node.kind() {
        SyntaxKind::MATCHER_IDENT => v.visit_matcher_ident(range, first_non_trivia_token(node)),

        SyntaxKind::MATCHER_WILDCARD => v.visit_matcher_wildcard(range),

        SyntaxKind::MATCHER_ALIAS => {
            let inner = walk_matcher(&children[0], v)?;
            let name = find_token(node, SyntaxKind::IDENT).expect("MATCHER_ALIAS has a name");
            v.visit_matcher_alias(range, inner, name)
        }

        SyntaxKind::MATCHER_TUPLE => {
            let mut items = Vec::with_capacity(children.len());
            for item in &children {
                items.push(walk_matcher(item, v)?);
            }
            v.visit_matcher_tuple(range, items)
        }

        SyntaxKind::MATCHER_OBJECT => {
            let exhaustive = !has_token(node, SyntaxKind::DOT_DOT_DOT);
            let mut fields = Vec::with_capacity(children.len());
            for field in &children {
                fields.push(extract_object_matcher_field(field, v)?);
            }
            v.visit_matcher_object(range, exhaustive, fields)
        }

        other => unreachable!("unexpected matcher node kind {other:?}"),
    }
}
