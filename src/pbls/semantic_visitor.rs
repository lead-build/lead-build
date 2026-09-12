use crate::pblang::{
    SyntaxToken,
    visit::{
        AssignKey, AttrSelector, LangVisitor, MapKind, ObjectField, StringPart, UnvisitedExpr,
        UnvisitedMatcher, visit_all,
    },
};
use rowan::TextRange;
use std::convert::Infallible;
use std::marker::PhantomData;
use std::rc::Rc;

/// Where a tracked identifier was bound. Anything not tracked by [`Scope`]
/// (object-literal keys, a matcher-object field's rename key, unresolved
/// references) has no `VarKind` and renders as a plain variable, exactly
/// as before this distinction existed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VarKind {
    LetBound,
    FuncArg,
}

/// A cons-list lexical environment: extending it (`bind_one`) never
/// touches the existing frames, just links a new one in front via `Rc`, so
/// repeatedly extending it down a sequence of bindings stays cheap.
#[derive(Clone, Default)]
pub struct Scope(Option<Rc<ScopeFrame>>);

struct ScopeFrame {
    name: String,
    kind: VarKind,
    /// Where this name was declared — the span of the identifier itself
    /// (the `MATCHER_IDENT`/`MATCHER_ALIAS`/shorthand-field name that
    /// introduced it), not the whole `let`/`bind`/`FUNC_DEF`. This is what
    /// lets a reference's `Entry` point straight back to its declaration
    /// site, e.g. for goto-definition.
    range: TextRange,
    parent: Scope,
}

impl Scope {
    fn bind_one(&self, name: &str, kind: VarKind, range: TextRange) -> Scope {
        Scope(Some(Rc::new(ScopeFrame {
            name: name.to_string(),
            kind,
            range,
            parent: self.clone(),
        })))
    }

    fn lookup(&self, name: &str) -> Option<(VarKind, TextRange)> {
        let mut frame = &self.0;
        while let Some(f) = frame {
            if f.name == name {
                return Some((f.kind, f.range));
            }
            frame = &f.parent.0;
        }
        None
    }
}

/// What a `Scope`-tracked declaration or reference carries in an [`Entry`],
/// parameterized so each `SemanticVisitor<T>` consumer only pays for the
/// data it actually needs instead of every consumer getting the union of
/// all of them (`VarKind`, *and* a `TextRange`, whether it wants the range
/// or not). `resolved` is called with the `VarKind` and declaration-site
/// `TextRange` `Scope` already tracked for a name — the same two pieces of
/// data regardless of consumer, just projected differently:
/// - `semantic_tokens.rs` colors tokens by `VarKind` alone → `T = VarKind`.
/// - `diagnostics.rs` only asks "did this resolve at all?" (`Option::is_none`)
///   → `T = ()`.
/// - `goto_definition.rs` needs to jump to the declaration →
///   `T = (VarKind, TextRange)`.
pub trait Resolved {
    fn resolved(kind: VarKind, range: TextRange) -> Self;
}

impl Resolved for VarKind {
    fn resolved(kind: VarKind, _range: TextRange) -> Self {
        kind
    }
}

impl Resolved for () {
    fn resolved(_kind: VarKind, _range: TextRange) -> Self {}
}

impl Resolved for (VarKind, TextRange) {
    fn resolved(kind: VarKind, range: TextRange) -> Self {
        (kind, range)
    }
}

/// One classified identifier: its span, whether it's a binding target or
/// property name (`true`) or a variable reference (`false`), and — only for
/// identifiers [`Scope`] actually tracks — the consumer-chosen `T` built
/// from its `VarKind` and declaration site (see [`Resolved`]). For a
/// declaration entry itself, that declaration site is simply its own span
/// (self-pointing): callers that only care about "which kind" (e.g.
/// semantic-token coloring) can ignore it, and callers that resolve a name
/// to its declaration (e.g. goto-definition) then don't need to
/// special-case "the cursor is already on the declaration".
///
/// `(true, None)` doesn't necessarily mean a binding target: an
/// object-literal `ASSIGNMENT` key, an `OBJECT_MATCHER_FIELD`'s rename-form
/// key, and a static `ATTR_SEL` right-hand side all name a *property*
/// instead — `Scope` never tracks any of them — and share that same
/// encoding, since consumers only need to tell "not a variable" apart from
/// "unresolved variable reference" (`(false, None)`, produced only by
/// `visit_var`).
type Entry<T> = (TextRange, bool, Option<T>);

fn concat<T>(mut a: Vec<Entry<T>>, b: Vec<Entry<T>>) -> Vec<Entry<T>> {
    a.extend(b);
    a
}

/// Walks the tree collecting `Entry<T>`s, generic over what a resolved
/// entry carries (see [`Resolved`]) so each consumer instantiates just the
/// shape it needs — e.g. `SemanticVisitor::<VarKind>::default()`.
pub struct SemanticVisitor<T>(PhantomData<T>);

// Written by hand rather than `#[derive(Default)]`: the derive would add a
// spurious `T: Default` bound (the marker field is `PhantomData<T>`, which
// is `Default` for every `T`, but the derive macro can't see that).
impl<T> Default for SemanticVisitor<T> {
    fn default() -> Self {
        SemanticVisitor(PhantomData)
    }
}

impl<T: Resolved> LangVisitor for SemanticVisitor<T> {
    type Expr = Vec<Entry<T>>;
    /// Entries this matcher subtree can already fully resolve (from
    /// default expressions, and rename keys — see `visit_matcher_object`),
    /// plus the names this matcher subtree itself binds. A matcher can't
    /// classify its own bound names as `LetBound` or `FuncArg` — that's a
    /// property of where it's attached, not of the pattern itself — so it
    /// reports them upward for `visit_let`/`visit_func_def` to classify and
    /// fold into `Scope`.
    type Matcher = (Vec<Entry<T>>, Vec<(TextRange, String)>);
    type Error = Infallible;
    type Down = Scope;

    fn visit_let(
        &mut self,
        _range: TextRange,
        down: &Scope,
        bindings: Vec<(TextRange, UnvisitedMatcher, UnvisitedExpr)>,
        body: UnvisitedExpr,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        let mut out = Vec::new();
        let mut scope = down.clone();
        for (_, matcher, value) in bindings {
            // Sequential: this binding's value sees only prior bindings.
            out.extend(value.visit(self, &scope)?);
            let (entries, names) = matcher.visit(self, &scope)?;
            out.extend(entries);
            for (range, name) in names {
                out.push((range, true, Some(T::resolved(VarKind::LetBound, range))));
                scope = scope.bind_one(&name, VarKind::LetBound, range);
            }
        }
        out.extend(body.visit(self, &scope)?);
        Ok(out)
    }

    fn visit_bind(
        &mut self,
        _range: TextRange,
        down: &Scope,
        items: Vec<(TextRange, AssignKey, UnvisitedExpr)>,
        body: UnvisitedExpr,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        let mut out = Vec::new();
        let mut scope = down.clone();
        for (_, key, value) in items {
            // Unlike `let`, bind items don't see each other — visited
            // against the unmodified incoming scope, not the growing one.
            out.extend(value.visit(self, down)?);
            if let AssignKey::Ident(token) = key {
                let range = token.text_range();
                out.push((range, true, Some(T::resolved(VarKind::LetBound, range))));
                scope = scope.bind_one(token.text(), VarKind::LetBound, range);
            }
        }
        out.extend(body.visit(self, &scope)?);
        Ok(out)
    }

    fn visit_func_def(
        &mut self,
        _range: TextRange,
        down: &Scope,
        params: Vec<UnvisitedMatcher>,
        body: UnvisitedExpr,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        let mut out = Vec::new();
        let mut scope = down.clone();
        for param in params {
            // Sequential: a later param's own default expressions can see
            // an earlier param's bound names.
            let (entries, names) = param.visit(self, &scope)?;
            out.extend(entries);
            for (range, name) in names {
                out.push((range, true, Some(T::resolved(VarKind::FuncArg, range))));
                scope = scope.bind_one(&name, VarKind::FuncArg, range);
            }
        }
        out.extend(body.visit(self, &scope)?);
        Ok(out)
    }

    fn visit_binary(
        &mut self,
        _range: TextRange,
        down: &Scope,
        _op: SyntaxToken,
        lhs: UnvisitedExpr,
        rhs: UnvisitedExpr,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        Ok(concat(lhs.visit(self, down)?, rhs.visit(self, down)?))
    }

    fn visit_unary(
        &mut self,
        _range: TextRange,
        down: &Scope,
        _op: SyntaxToken,
        operand: UnvisitedExpr,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        operand.visit(self, down)
    }

    fn visit_func_call(
        &mut self,
        _range: TextRange,
        down: &Scope,
        func: UnvisitedExpr,
        arg: UnvisitedExpr,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        Ok(concat(func.visit(self, down)?, arg.visit(self, down)?))
    }

    fn visit_attr_sel(
        &mut self,
        _range: TextRange,
        down: &Scope,
        base: UnvisitedExpr,
        attr: AttrSelector<UnvisitedExpr>,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        let base = base.visit(self, down)?;
        let attr = match attr {
            AttrSelector::Dynamic(entries) => entries.visit(self, down)?,
            // `.foo`: a property name, not a variable reference — same
            // `(true, None)` encoding as an object-literal or
            // object-matcher-field key (see `Entry`'s doc comment above).
            AttrSelector::Static(token) => vec![(token.text_range(), true, None)],
        };
        Ok(concat(base, attr))
    }

    fn visit_fold(
        &mut self,
        _range: TextRange,
        down: &Scope,
        func: UnvisitedExpr,
        init: Option<UnvisitedExpr>,
        input: UnvisitedExpr,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        let mut out = func.visit(self, down)?;
        if let Some(init) = init {
            out.extend(init.visit(self, down)?);
        }
        out.extend(input.visit(self, down)?);
        Ok(out)
    }

    fn visit_map(
        &mut self,
        _range: TextRange,
        down: &Scope,
        _kind: MapKind,
        func: UnvisitedExpr,
        input: UnvisitedExpr,
        filter: Option<UnvisitedExpr>,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        let mut out = func.visit(self, down)?;
        out.extend(input.visit(self, down)?);
        if let Some(filter) = filter {
            out.extend(filter.visit(self, down)?);
        }
        Ok(out)
    }

    fn visit_switch(
        &mut self,
        _range: TextRange,
        down: &Scope,
        input: UnvisitedExpr,
        cases: Vec<(UnvisitedExpr, UnvisitedExpr)>,
        default: Option<UnvisitedExpr>,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        let mut out = input.visit(self, down)?;
        for (pattern, result) in cases {
            out.extend(pattern.visit(self, down)?);
            out.extend(result.visit(self, down)?);
        }
        if let Some(default) = default {
            out.extend(default.visit(self, down)?);
        }
        Ok(out)
    }

    fn visit_object(
        &mut self,
        _range: TextRange,
        down: &Scope,
        items: Vec<(TextRange, AssignKey, UnvisitedExpr)>,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        let mut out = Vec::new();
        for (_, key, value) in items {
            if let AssignKey::Ident(token) = key {
                out.push((token.text_range(), true, None));
            }
            out.extend(value.visit(self, down)?);
        }
        Ok(out)
    }

    fn visit_list(
        &mut self,
        _range: TextRange,
        down: &Scope,
        items: Vec<UnvisitedExpr>,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        Ok(visit_all(items, self, down)?
            .into_iter()
            .flatten()
            .collect())
    }

    fn visit_tuple(
        &mut self,
        _range: TextRange,
        down: &Scope,
        items: Vec<UnvisitedExpr>,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        Ok(visit_all(items, self, down)?
            .into_iter()
            .flatten()
            .collect())
    }

    fn visit_literal(
        &mut self,
        _range: TextRange,
        _down: &Scope,
        _token: SyntaxToken,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        Ok(Vec::new())
    }

    fn visit_string(
        &mut self,
        _range: TextRange,
        down: &Scope,
        parts: Vec<StringPart<UnvisitedExpr>>,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        let mut out = Vec::new();
        for part in parts {
            if let StringPart::Embed(entries) = part {
                out.extend(entries.visit(self, down)?);
            }
        }
        Ok(out)
    }

    fn visit_var(
        &mut self,
        _range: TextRange,
        down: &Scope,
        name: SyntaxToken,
    ) -> Result<Vec<Entry<T>>, Infallible> {
        let kind = down
            .lookup(name.text())
            .map(|(kind, range)| T::resolved(kind, range));
        Ok(vec![(name.text_range(), false, kind)])
    }

    fn visit_matcher_ident(
        &mut self,
        range: TextRange,
        _down: &Scope,
        name: SyntaxToken,
    ) -> Result<(Vec<Entry<T>>, Vec<(TextRange, String)>), Infallible> {
        Ok((Vec::new(), vec![(range, name.text().to_string())]))
    }

    fn visit_matcher_wildcard(
        &mut self,
        _range: TextRange,
        _down: &Scope,
    ) -> Result<(Vec<Entry<T>>, Vec<(TextRange, String)>), Infallible> {
        Ok((Vec::new(), Vec::new()))
    }

    fn visit_matcher_alias(
        &mut self,
        _range: TextRange,
        down: &Scope,
        inner: UnvisitedMatcher,
        name: SyntaxToken,
    ) -> Result<(Vec<Entry<T>>, Vec<(TextRange, String)>), Infallible> {
        let (entries, mut names) = inner.visit(self, down)?;
        names.push((name.text_range(), name.text().to_string()));
        Ok((entries, names))
    }

    fn visit_matcher_tuple(
        &mut self,
        _range: TextRange,
        down: &Scope,
        items: Vec<UnvisitedMatcher>,
    ) -> Result<(Vec<Entry<T>>, Vec<(TextRange, String)>), Infallible> {
        let mut all_entries = Vec::new();
        let mut all_names = Vec::new();
        for item in items {
            let (entries, names) = item.visit(self, down)?;
            all_entries.extend(entries);
            all_names.extend(names);
        }
        Ok((all_entries, all_names))
    }

    fn visit_matcher_object(
        &mut self,
        _range: TextRange,
        down: &Scope,
        _exhaustive: bool,
        fields: Vec<ObjectField<UnvisitedMatcher, UnvisitedExpr>>,
    ) -> Result<(Vec<Entry<T>>, Vec<(TextRange, String)>), Infallible> {
        let mut all_entries = Vec::new();
        let mut all_names = Vec::new();
        for field in fields {
            match field.matcher {
                // Renamed form (`{a = pattern}`): `key` names a property of
                // the source object, not a new variable — the pattern's own
                // names are what gets bound.
                Some(matcher) => {
                    all_entries.push((field.key.text_range(), true, None));
                    let (entries, names) = matcher.visit(self, down)?;
                    all_entries.extend(entries);
                    all_names.extend(names);
                }
                // Shorthand form (`{foo}`): `key`'s text is itself the
                // bound variable's name.
                None => {
                    all_names.push((field.key.text_range(), field.key.text().to_string()));
                }
            }
            if let Some(default) = field.default {
                // Sees the scope outside this matcher, never a sibling
                // field bound by it.
                all_entries.extend(default.visit(self, down)?);
            }
        }
        Ok((all_entries, all_names))
    }
}
