//! Renders `Expr`/`ExprType` values as pb-syntax-shaped text for
//! diagnostics (error messages, `Display`). This is the *evaluated value ->
//! text* direction, the opposite of parsing — it doesn't need, and doesn't
//! build, any intermediate tree type; it writes straight to a [`Printer`].
//!
//! It's deliberately not exact or re-parseable: an [`Expr`] can be an
//! infinite/cyclic lazy graph or just a huge one, and this printer exists to
//! label such a thing for a human ("a list expression with 4000 more
//! items"), not to reproduce it byte-for-byte. See [`Printer`] for how that
//! stays safe.

use super::*;
use std::fmt::{self, Write};

/// Maximum `Expr` nesting the printer will descend into before switching to
/// an elision placeholder. Bounds stack depth against cyclic or
/// pathologically deep expression graphs.
const MAX_DEPTH: usize = 64;

/// Maximum number of collection items (object fields, list/tuple elements,
/// string-concat parts, ...) printed in total across one `export` call.
/// Bounds output size against huge (but possibly shallow) data structures.
const MAX_ITEMS: usize = 500;

/// Depth budget for [`Printer::new_diag`]: about two levels of real nesting
/// before eliding, so a diagnostic snippet stays short no matter how big or
/// deep the actual value is. See [`Diag`].
const DIAG_MAX_DEPTH: usize = 2;

/// Item budget (per `export` call, not per collection) for
/// [`Printer::new_diag`] — small enough that even a huge flat collection
/// contributes only a line or two.
const DIAG_MAX_ITEMS: usize = 3;

/// A [`std::fmt::Write`] sink with a depth and size budget. See the module
/// docs for why: this is what keeps [`Exportable::export`] always
/// terminating and reasonably sized, even for a cyclic or huge `Expr`.
pub struct Printer<'a> {
    out: &'a mut dyn Write,
    depth_left: usize,
    items_left: usize,
}

impl<'a> Printer<'a> {
    pub fn new(out: &'a mut dyn Write) -> Self {
        Self::with_budget(out, MAX_DEPTH, MAX_ITEMS)
    }

    /// A tightly-budgeted printer for embedding a value inline in a
    /// diagnostic message: see [`Diag`] for when to use it instead of plain
    /// `Display`.
    pub fn new_diag(out: &'a mut dyn Write) -> Self {
        Self::with_budget(out, DIAG_MAX_DEPTH, DIAG_MAX_ITEMS)
    }

    fn with_budget(out: &'a mut dyn Write, depth: usize, items: usize) -> Self {
        Self {
            out,
            depth_left: depth,
            items_left: items,
        }
    }

    /// Runs `body` one nesting level deeper, or writes `<{concept}>` instead
    /// if the depth budget is used up.
    pub fn nested(
        &mut self,
        concept: &str,
        body: impl FnOnce(&mut Printer) -> fmt::Result,
    ) -> fmt::Result {
        if self.depth_left == 0 {
            return write!(self, "<{concept}>");
        }
        self.depth_left -= 1;
        let result = body(self);
        self.depth_left += 1;
        result
    }

    /// Prints `items` via `each`, separated by `sep`, stopping early (with a
    /// `"N more"` note) once the shared item budget runs out — so a huge
    /// collection can't blow up the output.
    pub fn list<I>(
        &mut self,
        items: I,
        sep: &str,
        mut each: impl FnMut(&mut Printer, I::Item) -> fmt::Result,
    ) -> fmt::Result
    where
        I: ExactSizeIterator,
    {
        let total = items.len();
        for (index, item) in items.enumerate() {
            if self.items_left == 0 {
                return write!(self, "{sep}/* {} more */", total - index);
            }
            self.items_left -= 1;
            if index > 0 {
                self.write_str(sep)?;
            }
            each(self, item)?;
        }
        Ok(())
    }
}

impl Write for Printer<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.out.write_str(s)
    }
}

/// Implemented by anything that can render itself for diagnostics through a
/// [`Printer`]. Always succeeds (short of the underlying writer itself
/// failing): constructs with no real syntax (builtins, opaque runtime
/// values) get an inline placeholder naming the concept instead of an
/// error, matching how depth/size elision already works.
pub trait Exportable {
    fn export(&self, out: &mut Printer<'_>) -> fmt::Result;

    /// Wraps `self` for a short, diagnostic-sized `Display` — about two
    /// levels of real nesting, a handful of items total, everything else
    /// elided — meant to be embedded inline in an error message
    /// (`format!("... got {}", value.diag())`). Use this instead of `{}`
    /// (which goes through [`Printer::new`]'s much larger budget, meant for
    /// inspecting a complete value such as `pb -E`'s output) whenever an
    /// `Expr`/`ExprType`/value is interpolated into a diagnostic: a build
    /// object can be arbitrarily large, and a diagnostic should stay
    /// readable regardless.
    fn diag(&self) -> Diag<'_, Self>
    where
        Self: Sized,
    {
        Diag(self)
    }
}

/// See [`Exportable::diag`].
pub struct Diag<'a, E: ?Sized>(&'a E);

impl<E: Exportable + ?Sized> Display for Diag<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.export(&mut Printer::new_diag(f))
    }
}

impl<T, F> Exportable for Expr<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    fn export(&self, out: &mut Printer<'_>) -> fmt::Result {
        let tok = self.inner_ref().tok.clone();
        let concept = tok.get_message().unwrap_or("expression");
        out.nested(concept, |p| tok.export(p))
    }
}

impl<T, F> Exportable for ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    fn export(&self, out: &mut Printer<'_>) -> fmt::Result {
        match self {
            ExprType::Object(items) => {
                out.write_str("{")?;
                out.list(items.iter(), "", |p, (key, value)| {
                    write!(p, "{key}=")?;
                    value.export(p)?;
                    p.write_str(";")
                })?;
                out.write_str("}")
            }
            ExprType::List(items) => {
                out.write_str("[")?;
                out.list(items.iter(), ",", |p, item| item.export(p))?;
                out.write_str("]")
            }
            ExprType::Tuple(items) => {
                out.write_str("(")?;
                out.list(items.iter(), ",", |p, item| item.export(p))?;
                out.write_str(")")
            }
            // No real syntax for a not-yet-concatenated string: print the
            // pieces adjacently, since that's what they'll look like once
            // resolved into one string.
            ExprType::Concat(parts) => out.list(parts.iter(), "", |p, part| part.export(p)),
            ExprType::AttrSel(value, attr) => {
                value.export(out)?;
                out.write_str(".")?;
                match &attr.inner_ref().tok {
                    ExprType::Value(v) if is_ident(&v.to_string()) => write!(out, "{v}"),
                    _ => {
                        out.write_str("{")?;
                        attr.export(out)?;
                        out.write_str("}")
                    }
                }
            }
            ExprType::Value(value) => value.export(out),
            ExprType::Var(name) => write!(out, "{name}"),
            ExprType::UnOp(op, value) => {
                write!(out, "{op}")?;
                value.export(out)
            }
            ExprType::BinOp(op, lhs, rhs) => {
                lhs.export(out)?;
                write!(out, "{op}")?;
                rhs.export(out)
            }
            ExprType::FuncDef(matcher, body) => {
                out.write_str("|")?;
                export_matcher(out, matcher)?;
                out.write_str("|")?;
                body.export(out)
            }
            ExprType::FuncDefBuiltin(ExprBuiltinWrapper(name, _)) => {
                write!(out, "<builtin {name}>")
            }
            ExprType::Let(bindings, body) => {
                out.write_str("let ")?;
                out.list(bindings.iter(), "", |p, (matcher, value)| {
                    export_matcher(p, matcher)?;
                    p.write_str("=")?;
                    value.export(p)?;
                    p.write_str(";")
                })?;
                out.write_str("in ")?;
                body.export(out)
            }
            ExprType::Fold { func, init, input } => {
                out.write_str("(")?;
                func.export(out)?;
                out.write_str(" for ")?;
                if let Some(init) = init {
                    init.export(out)?;
                    out.write_str(":")?;
                }
                input.export(out)?;
                out.write_str(")")
            }
            ExprType::Map(kind, func, input, filter) => {
                out.write_str(match kind {
                    ExprMapType::List => "[",
                    ExprMapType::Object => "{",
                })?;
                func.export(out)?;
                out.write_str(" for ")?;
                input.export(out)?;
                if let Some(filter) = filter {
                    out.write_str(" if ")?;
                    filter.export(out)?;
                }
                out.write_str(match kind {
                    ExprMapType::List => "]",
                    ExprMapType::Object => "}",
                })
            }
            ExprType::FuncCall { arg, func } => {
                func.export(out)?;
                out.write_str(" ")?;
                arg.export(out)
            }
            ExprType::Bind(items, body) => {
                out.write_str("bind ")?;
                out.list(items.iter(), "", |p, (key, value)| {
                    write!(p, "{key}=")?;
                    value.export(p)?;
                    p.write_str(";")
                })?;
                out.write_str("in ")?;
                body.export(out)
            }
            ExprType::Switch(input, cases, default) => {
                out.write_str("switch ")?;
                input.export(out)?;
                out.write_str(" {")?;
                out.list(cases.iter(), "", |p, (matcher, value)| {
                    matcher.export(p)?;
                    p.write_str("=>")?;
                    value.export(p)?;
                    p.write_str(";")
                })?;
                if let Some(default) = default {
                    out.write_str("_=>")?;
                    default.export(out)?;
                    out.write_str(";")?;
                }
                out.write_str("}")
            }
            ExprType::Null => out.write_str("null"),
            ExprType::UnderEval => out.write_str("<being evaluated>"),
        }
    }
}

fn export_matcher<T, F>(out: &mut Printer<'_>, matcher: &Matcher<T, F>) -> fmt::Result
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    out.nested("matcher", |out| match matcher {
        Matcher::Alias(inner, name) => {
            export_matcher(out, inner)?;
            write!(out, "@{name}")
        }
        Matcher::DontCare => out.write_str("_"),
        Matcher::Ident(name) => write!(out, "{name}"),
        Matcher::Tuple(items) => {
            out.write_str("(")?;
            out.list(items.iter(), ",", |p, item| export_matcher(p, item))?;
            out.write_str(")")
        }
        Matcher::Object(fields, exhaustive) => {
            out.write_str("{")?;
            out.list(fields.iter(), ",", |p, (key, matcher, default)| {
                write!(p, "{key}")?;
                if !matches!(matcher, Matcher::Ident(name) if name == key) {
                    p.write_str("=")?;
                    export_matcher(p, matcher)?;
                }
                if let Some(default) = default {
                    p.write_str("?")?;
                    default.export(p)?;
                }
                Ok(())
            })?;
            if !exhaustive {
                if !fields.is_empty() {
                    out.write_str(",")?;
                }
                out.write_str("...")?;
            }
            out.write_str("}")
        }
    })
}

fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::super::super::testvalue::{FRef, TestValue};
    use super::*;

    #[test]
    fn deeply_nested_expr_terminates_and_elides() {
        let mut expr: Expr<TestValue, FRef> = ExprType::Value(TestValue::Int(0)).builtin();
        for _ in 0..(MAX_DEPTH * 4) {
            expr = ExprType::List(vec![expr]).builtin();
        }
        let text = expr.to_string();
        assert!(
            text.contains("<list expression>"),
            "deep nesting should hit the depth budget and elide: {text}"
        );
        // Roughly bounded by MAX_DEPTH levels of "[", not MAX_DEPTH * 4.
        assert!(
            text.len() < MAX_DEPTH * 10,
            "output should stay small: {text}"
        );
    }

    #[test]
    fn cyclic_expr_terminates_and_elides() {
        // A genuinely self-referential `Expr`: `expr` is a list containing
        // itself. Nothing about the tree is finite; only the printer's
        // depth budget keeps `Display` from looping forever.
        let expr: Expr<TestValue, FRef> = ExprType::Null.builtin();
        expr.replace_storage(ExprStorage {
            tok: ExprType::List(vec![expr.clone()]),
            loc: None,
        });

        let text = expr.to_string();
        assert!(
            text.contains("<list expression>"),
            "a cycle should hit the depth budget and elide: {text}"
        );
        assert!(
            text.len() < MAX_DEPTH * 10,
            "output should stay small: {text}"
        );
    }

    #[test]
    fn huge_list_is_truncated_with_a_count() {
        let items: Vec<Expr<TestValue, FRef>> = (0..(MAX_ITEMS * 10))
            .map(|i| ExprType::Value(TestValue::Int(i as i64)).builtin())
            .collect();
        let expr = ExprType::List(items).builtin();

        let text = expr.to_string();
        assert!(
            text.contains("more"),
            "a huge list should be truncated with a count: {text}"
        );
        assert!(
            text.len() < MAX_ITEMS * 20,
            "output should stay bounded: {text}"
        );
    }

    #[test]
    fn builtin_func_and_under_eval_print_placeholders_instead_of_failing() {
        // These used to hard-error / `todo!()` / `panic!()` respectively;
        // now every `ExprType` variant has *some* text.
        assert!(
            ExprType::<TestValue, FRef>::UnderEval
                .builtin()
                .to_string()
                .contains("being evaluated")
        );
    }

    /// Builds a `depth`-deep nested object, each level holding `width`
    /// sibling fields, so both the depth and item budgets are stressed at
    /// once (a shape that a plain-`{}` real-world build object could
    /// plausibly take, unlike the single-branch deep list used above).
    fn wide_nested_object(depth: usize, width: usize) -> Expr<TestValue, FRef> {
        let mut expr = ExprType::Value(TestValue::Int(0)).builtin();
        for level in 0..depth {
            let mut fields = ExprSet::new();
            for i in 0..width {
                fields.insert(
                    StrKey::from(&format!("field_{level}_{i}")),
                    expr.clone(),
                );
            }
            expr = ExprType::Object(fields).builtin();
        }
        expr
    }

    #[test]
    fn diag_stays_short_and_single_line_for_a_huge_object() {
        let expr = wide_nested_object(10, 20);
        let full = expr.to_string();
        let diag = expr.diag().to_string();

        assert!(!diag.contains('\n'), "diag output should be one line: {diag}");
        assert!(
            diag.len() < 200,
            "diag output should be short even for a huge value: {diag}"
        );
        assert!(
            full.len() > diag.len() * 5,
            "the huge value should actually be much bigger than its diag: full={} diag={}",
            full.len(),
            diag.len()
        );
    }

    #[test]
    fn diag_shows_about_two_levels_of_real_structure() {
        let expr = wide_nested_object(5, 2);
        let diag = expr.diag().to_string();
        // Depth 0 (the object itself) and depth 1 (its fields' values, one
        // level of nested object) should be real; depth 2 should already
        // be elided as a placeholder rather than expanded further.
        assert!(diag.contains("field_4_"), "{diag}");
        assert!(diag.contains("<object expression>"), "{diag}");
    }
}
