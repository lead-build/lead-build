mod export;
pub mod matcher;

use super::error::{Error, ErrorType, Loc, Referrable, Result, Span};
use super::suggest::{closest, name_list};
use crate::strkey::StrKey;
pub use export::{Diag, Exportable, Printer};
pub use matcher::Matcher;
use std::{
    cell::{Ref, RefCell},
    collections::{BTreeMap, HashSet},
    fmt::{Debug, Display},
    rc::Rc,
};
use strum::{EnumMessage, EnumTryAs};

use log::trace;

#[cfg(test)]
mod tests;

pub trait ExprOps<F>: Sized {
    fn op_add(lhs: &Self, rhs: &Self) -> Result<Self, F>;
    fn op_sub(lhs: &Self, rhs: &Self) -> Result<Self, F>;
    fn op_mult(lhs: &Self, rhs: &Self) -> Result<Self, F>;
    fn op_div(lhs: &Self, rhs: &Self) -> Result<Self, F>;
    fn op_string_concat(parts: Vec<Self>) -> Result<Self, F>;
    fn op_lt(lhs: &Self, rhs: &Self) -> Result<Self, F>;
    fn op_le(lhs: &Self, rhs: &Self) -> Result<Self, F>;
    fn op_gt(lhs: &Self, rhs: &Self) -> Result<Self, F>;
    fn op_ge(lhs: &Self, rhs: &Self) -> Result<Self, F>;
    fn op_eq(lhs: &Self, rhs: &Self) -> Result<Self, F>;
    fn op_neq(lhs: &Self, rhs: &Self) -> Result<Self, F>;
    fn op_neg(&self) -> Result<Self, F>;
    fn op_not(&self) -> Result<Self, F>;
    fn as_bool(&self) -> Result<bool, F>;
    fn as_string(&self) -> Result<String, F>;
    fn new_from_bool(value: bool) -> Self;
    fn new_from_string(value: impl ToString) -> Self;
    /// What kind of value this is, in the words of an error message, with
    /// its article: "a string", "an integer".
    fn type_name(&self) -> &'static str;
}

pub trait ExprBuiltin<T, F>: Debug
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    fn get_name(&self) -> StrKey;
    fn call(&self, arg: Expr<T, F>) -> Result<Expr<T, F>, F>;
}

/* *****************************************************************************
 * Types
 */

#[derive(Debug, PartialEq, Clone)]
pub struct Expr<T, F>(Rc<RefCell<ExprStorage<T, F>>>)
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone;

// TODO: Better implementation of ExprSet... This probably takes time to clone.
pub type ExprSet<T, F> = BTreeMap<StrKey, Expr<T, F>>;

/// The most rewrite steps one expression may take to resolve before it is
/// reported as an infinite recursion, see [`Expr::resolve`].
const MAX_RESOLVE_STEPS: usize = 250_000;

/// The most fields an object may have for an error to list them all.
const MAX_LISTED_FIELDS: usize = 8;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ExprBinOp {
    HasAttr,
    ListConcat,
    Mult,
    Div,
    Sub,
    Add,
    Update,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Neq,
    LogAnd,
    LogOr,
    LogImpl,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ExprUnOp {
    Neg,
    Not,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ExprMapType {
    List,
    Object,
}

#[derive(Clone)]
pub struct ExprBuiltinWrapper<T, F>(StrKey, Rc<dyn ExprBuiltin<T, F>>)
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone;

#[derive(Debug, Clone)]
pub struct ExprStorage<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    pub tok: ExprType<T, F>,
    pub loc: Option<Loc<F>>,
}

// Clone is needed since ExprType::Var is implemented via cloning of ExprType
#[derive(Debug, PartialEq, Clone, Default, EnumTryAs, EnumMessage)]
pub enum ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    #[strum(message = "object expression")]
    Object(ExprSet<T, F>),
    #[strum(message = "list expression")]
    List(Vec<Expr<T, F>>),
    #[strum(message = "tuple expression")]
    Tuple(Vec<Expr<T, F>>),
    #[strum(message = "concat expression")]
    Concat(Vec<Expr<T, F>>),
    #[strum(message = "attribute selection")]
    AttrSel(Expr<T, F>, Expr<T, F>),
    #[strum(message = "literal value")]
    Value(T),
    #[strum(message = "variable reference")]
    Var(StrKey),
    #[strum(message = "unary operator expression")]
    UnOp(ExprUnOp, Expr<T, F>),
    #[strum(message = "binary operator expression")]
    BinOp(ExprBinOp, Expr<T, F>, Expr<T, F>),
    #[strum(message = "function definition")]
    FuncDef(Matcher<T, F>, Expr<T, F>),
    #[strum(message = "builtin function definition")]
    FuncDefBuiltin(ExprBuiltinWrapper<T, F>),
    #[strum(message = "let binding")]
    Let(Vec<(Matcher<T, F>, Expr<T, F>)>, Expr<T, F>),
    #[strum(message = "fold expression")]
    Fold {
        func: Expr<T, F>,
        init: Option<Expr<T, F>>,
        input: Expr<T, F>,
    },
    #[strum(message = "map expression")]
    Map(ExprMapType, Expr<T, F>, Expr<T, F>, Option<Expr<T, F>>),
    #[strum(message = "function call")]
    FuncCall { arg: Expr<T, F>, func: Expr<T, F> },
    #[strum(message = "bound expression")]
    Bind(ExprSet<T, F>, Expr<T, F>),
    #[strum(message = "switch expression")]
    Switch(
        Expr<T, F>,
        Vec<(Expr<T, F>, Expr<T, F>)>,
        Option<Expr<T, F>>,
    ),
    #[strum(message = "match expression")]
    Match(Expr<T, F>, Vec<(Matcher<T, F>, Expr<T, F>)>),
    #[strum(message = "null expression")]
    Null,
    #[default]
    #[strum(message = "under evaluation")]
    UnderEval,
}

/* *****************************************************************************
 * Drop
 */

impl<T, F> ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    /// Move the directly contained expressions out, for dropping them.
    /// Expressions inside matchers are left to the matcher's own drop, as
    /// they only nest as deep as the source.
    fn take_children(self, out: &mut Vec<Expr<T, F>>) {
        match self {
            ExprType::Object(fields) => out.extend(fields.into_values()),
            ExprType::List(items) | ExprType::Tuple(items) | ExprType::Concat(items) => {
                out.extend(items)
            }
            ExprType::AttrSel(a, b) | ExprType::BinOp(_, a, b) => out.extend([a, b]),
            ExprType::UnOp(_, e) | ExprType::FuncDef(_, e) => out.push(e),
            ExprType::Let(bindings, body) => {
                out.extend(bindings.into_iter().map(|(_, e)| e));
                out.push(body);
            }
            ExprType::Fold { func, init, input } => {
                out.extend([func, input]);
                out.extend(init);
            }
            ExprType::Map(_, func, input, filter) => {
                out.extend([func, input]);
                out.extend(filter);
            }
            ExprType::FuncCall { arg, func } => out.extend([arg, func]),
            ExprType::Bind(vars, e) => {
                out.extend(vars.into_values());
                out.push(e);
            }
            ExprType::Switch(e, cases, default) => {
                out.push(e);
                for (m, c) in cases {
                    out.extend([m, c]);
                }
                out.extend(default);
            }
            ExprType::Match(e, cases) => {
                out.push(e);
                out.extend(cases.into_iter().map(|(_, c)| c));
            }
            ExprType::Value(_)
            | ExprType::Var(_)
            | ExprType::FuncDefBuiltin(_)
            | ExprType::Null
            | ExprType::UnderEval => {}
        }
    }
}

impl<T, F> Drop for Expr<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    /// Drop the expression tree iteratively. Lazy evaluation builds long
    /// chains of cells, such as the unused argument of each step of a
    /// tail-recursive call, and the default recursive drop would overflow
    /// the stack on them. Each cell is emptied here before it is dropped, so
    /// the nested drop never has children to recurse into.
    fn drop(&mut self) {
        /// Take the storage out of a cell about to be freed, leaving an
        /// empty one behind for the actual drop.
        fn take_last<T, F>(expr: &Expr<T, F>) -> Option<ExprStorage<T, F>>
        where
            T: Clone + PartialEq + Display + ExprOps<F>,
            F: Clone,
        {
            if Rc::strong_count(&expr.0) != 1 {
                return None;
            }
            expr.0
                .try_borrow_mut()
                .ok()
                .map(|mut storage| std::mem::take(&mut *storage))
        }

        let Some(storage) = take_last(self) else {
            return;
        };
        let mut pending: Vec<Expr<T, F>> = Vec::new();
        storage.tok.take_children(&mut pending);
        while let Some(child) = pending.pop() {
            if let Some(storage) = take_last(&child) {
                storage.tok.take_children(&mut pending);
            }
            // `child` is dropped here, either empty or still shared
        }
    }
}

/* *****************************************************************************
 * PartialEq
 */

impl<T, F> PartialEq for ExprStorage<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone + PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.tok == other.tok
    }
}

/* *****************************************************************************
 * Location handling
 */

impl<T, F> Expr<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    pub fn get_loc(&self) -> Option<Loc<F>> {
        self.inner_ref().loc.clone()
    }

    /// Where the value of this expression was written, which is where a
    /// label about the value belongs: a variable or a field leads on to
    /// what it refers to.
    pub fn origin(&self) -> Option<Loc<F>> {
        self.get_loc().map(|loc| loc.origin().clone())
    }

    /// What kind of value this is, see [`ExprType::describe`].
    pub fn describe(&self) -> &'static str {
        self.0.as_ref().borrow().tok.describe()
    }
}

impl<T, F> ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    /// What kind of value this is, in the words of an error message, with
    /// its article: "a list", "a string", "null".
    pub fn describe(&self) -> &'static str {
        match self {
            ExprType::Object(..) => "an object",
            ExprType::List(..) => "a list",
            ExprType::Tuple(..) => "a tuple",
            ExprType::Value(value) => value.type_name(),
            ExprType::FuncDef(..) | ExprType::FuncDefBuiltin(..) => "a function",
            ExprType::Null => "null",
            _ => "an unevaluated expression",
        }
    }

    pub fn reref(self: ExprType<T, F>, loc: Option<Loc<F>>) -> Expr<T, F> {
        Expr(Rc::new(RefCell::new(ExprStorage { tok: self, loc })))
    }

    pub fn toexpr(self: ExprType<T, F>, start: usize, end: usize, f: &F) -> Expr<T, F> {
        self.reref(Some(Loc::new(f.clone(), Span { start, end })))
    }

    pub fn new_builtin(func: impl ExprBuiltin<T, F> + 'static) -> ExprType<T, F> {
        let func = Rc::new(func);
        ExprType::FuncDefBuiltin(ExprBuiltinWrapper(func.get_name(), func))
    }

    pub fn builtin(self: ExprType<T, F>) -> Expr<T, F> {
        self.reref(None)
    }

    pub fn loc(self: ExprType<T, F>, loc: Option<Loc<F>>) -> ExprStorage<T, F> {
        ExprStorage { tok: self, loc }
    }
}

/* *****************************************************************************
 * Display
 */

impl<T, F> Debug for ExprBuiltinWrapper<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ExprBuiltinWrapper").field(&self.0).finish()
    }
}

impl Display for ExprBinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExprBinOp::HasAttr => write!(f, "?"),
            ExprBinOp::ListConcat => write!(f, "++"),
            ExprBinOp::Mult => write!(f, "*"),
            ExprBinOp::Div => write!(f, "/"),
            ExprBinOp::Sub => write!(f, "-"),
            ExprBinOp::Add => write!(f, "+"),
            ExprBinOp::Update => write!(f, "//"),
            ExprBinOp::Lt => write!(f, "<"),
            ExprBinOp::Le => write!(f, "<="),
            ExprBinOp::Gt => write!(f, ">"),
            ExprBinOp::Ge => write!(f, ">="),
            ExprBinOp::Eq => write!(f, "=="),
            ExprBinOp::Neq => write!(f, "!="),
            ExprBinOp::LogAnd => write!(f, "&&"),
            ExprBinOp::LogOr => write!(f, "||"),
            ExprBinOp::LogImpl => write!(f, "->"),
        }
    }
}

impl Display for ExprUnOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExprUnOp::Neg => write!(f, "-"),
            ExprUnOp::Not => write!(f, "!"),
        }
    }
}

impl<T, F> Display for Expr<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Exportable::export(self, &mut Printer::new(f))
    }
}

impl<T, F> Display for ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Exportable::export(self, &mut Printer::new(f))
    }
}

/* *****************************************************************************
 * Transform / From
 */

impl<T, F> From<ExprStorage<T, F>> for Expr<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    fn from(value: ExprStorage<T, F>) -> Self {
        Expr(Rc::new(RefCell::new(value)))
    }
}

impl<T, F> From<ExprSet<T, F>> for ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    fn from(value: ExprSet<T, F>) -> Self {
        ExprType::Object(value)
    }
}

impl<T, F> From<T> for ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    fn from(value: T) -> Self {
        ExprType::Value(value)
    }
}

/* *****************************************************************************
 * Implementations
 */

impl<T, F> Default for ExprStorage<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    fn default() -> Self {
        Self {
            tok: Default::default(),
            loc: None,
        }
    }
}

impl<T, F> PartialEq for ExprBuiltinWrapper<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    fn eq(&self, other: &Self) -> bool {
        #[cfg(test)]
        {
            self.0 == other.0
        }
        #[cfg(not(test))]
        {
            let _ = other;
            unreachable!("== of builtin expressions should not be used")
        }
    }
}

impl<T, F> Expr<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    /// A cell that evaluates `self` in `varspace`.
    ///
    /// A plain variable reference is the cell it refers to, not a new cell
    /// wrapping the reference: a variable passed on unchanged through a
    /// recursion, as in `self.f (i - 1, sum)`, would otherwise leave a chain
    /// of one unresolved cell per step, forced recursively only when the
    /// value is finally used (see the `ok_recurse_tail_call_passthrough`
    /// fixture). The cell may therefore be shared with others, so the caller
    /// must only ever resolve it in place.
    ///
    /// A caller that takes the cell apart uses [`Expr::bind_fresh`]: one that
    /// takes over its storage as its own next step, and one that unpacks it
    /// into scope and body to add variables, as a call does with a function
    /// body and `match` with a case. Unpacking a shared cell would evaluate
    /// its expression with those added variables in scope, and they would
    /// capture any variable of the same name it refers to (the
    /// `ok_scope_*` fixtures).
    pub fn bind(&self, varspace: &ExprSet<T, F>) -> Expr<T, F> {
        if let ExprType::Var(name) = &self.inner_ref().tok
            && let Some(target) = varspace.get(name)
        {
            return target.clone();
        }
        self.bind_fresh(varspace)
    }

    /// A new cell that evaluates `self` in `varspace`, private to the
    /// caller, which may take it apart: take over its storage as the next
    /// step of the cell it is resolving, or unpack it into scope and body
    /// (a function body on a call, a let body, a case of `switch` or
    /// `match`, the start value of a fold).
    ///
    /// This is a bind step even when `self` is a plain variable: the shared
    /// cell behind it must be resolved in place, not copied or unpacked. A
    /// copy would be evaluated once more, or be the `UnderEval` marker of a
    /// cell that is being resolved further up.
    ///
    /// The whole varspace is cloned (cheap: keys are copied, values are
    /// reference counted). Narrowing it to the variables the expression
    /// refers to was tried and cost more than it saved.
    pub fn bind_fresh(&self, varspace: &ExprSet<T, F>) -> Expr<T, F> {
        ExprType::Bind(varspace.clone(), self.clone()).reref(self.get_loc())
    }

    pub fn inner_ref(&self) -> Ref<'_, ExprStorage<T, F>> {
        self.0.as_ref().borrow()
    }

    /// Point an error from a binary operation at the operation, and at
    /// where the value of each side was written: one of those is most
    /// often where the mistake is.
    fn binop_context(
        err: Error<F>,
        site: &Option<Loc<F>>,
        lhs: &Expr<T, F>,
        rhs: &Expr<T, F>,
    ) -> Error<F> {
        // A side can be the result of this very expression, from an earlier
        // step of a fold or a recursion. A label for it would sit on the
        // whole operation and read as if it described both sides.
        let side = |side: &Expr<T, F>| {
            side.origin()
                .filter(|origin| site.as_ref().is_none_or(|site| site.span != origin.span))
        };
        err.primary(site, "")
            .label(&side(lhs), format!("the left side is {}", lhs.describe()))
            .label(&side(rhs), format!("the right side is {}", rhs.describe()))
    }

    /// The error for an operator that does not apply to these two values.
    fn binop_error(
        op: ExprBinOp,
        site: &Option<Loc<F>>,
        lhs: &Expr<T, F>,
        rhs: &Expr<T, F>,
    ) -> Error<F> {
        Self::binop_context(
            Error::new(
                ErrorType::Type,
                format!(
                    "cannot apply `{}` to {} and {}",
                    op,
                    lhs.describe(),
                    rhs.describe()
                ),
            ),
            site,
            lhs,
            rhs,
        )
    }

    /// `site` is the location of the whole operation.
    fn resolve_binop(
        lhs: &Expr<T, F>,
        op: ExprBinOp,
        rhs: &Expr<T, F>,
        site: &Option<Loc<F>>,
    ) -> Result<ExprType<T, F>, F> {
        // An error from the values themselves, pointed at the operation
        let at_op = |err: Error<F>| Self::binop_context(err, site, lhs, rhs);
        // The same for an operator that decides on its left side alone; the
        // right side may not even be evaluated
        let at_left = |err: Error<F>| {
            err.primary(site, "").label(
                &lhs.origin(),
                format!("the left side is {}", lhs.describe()),
            )
        };
        let unsupported = || Self::binop_error(op, site, lhs, rhs);
        match op {
            ExprBinOp::LogAnd | ExprBinOp::LogOr | ExprBinOp::LogImpl => {
                // Operators that can be lazy
                let lhs_val = lhs.value().map_err(|e| e.reref(&lhs.get_loc()))?;
                let rhs_val = || rhs.value().map_err(|e| e.reref(&rhs.get_loc()));
                match op {
                    ExprBinOp::LogAnd => Ok(match lhs_val.as_bool().map_err(at_left)? {
                        true => ExprType::Value(rhs_val()?),
                        false => ExprType::Value(lhs_val),
                    }),
                    ExprBinOp::LogOr => Ok(match lhs_val.as_bool().map_err(at_left)? {
                        true => ExprType::Value(lhs_val),
                        false => ExprType::Value(rhs_val()?),
                    }),
                    ExprBinOp::LogImpl => Ok(match lhs_val.as_bool().map_err(at_left)? {
                        true => ExprType::Value(rhs_val()?),
                        false => ExprType::Value(T::new_from_bool(true)),
                    }),
                    _ => unreachable!(),
                }
            }
            _ => {
                let lhs_r = lhs.res_type().map_err(|e| e.reref(&lhs.get_loc()))?;
                let rhs_r = rhs.res_type().map_err(|e| e.reref(&rhs.get_loc()))?;

                fn list_eq<
                    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
                    F: Clone + Debug + Referrable,
                >(
                    lhs_els: &[Expr<T, F>],
                    rhs_els: &[Expr<T, F>],
                ) -> Result<ExprType<T, F>, F> {
                    if lhs_els.len() != rhs_els.len() {
                        Ok(ExprType::Value(T::new_from_bool(false)))
                    } else {
                        let mut all_equal: ExprType<T, F> = ExprType::Value(T::new_from_bool(true));
                        for (lhs_item, rhs_item) in lhs_els.iter().zip(rhs_els.iter()) {
                            let this_equal =
                                ExprType::BinOp(ExprBinOp::Eq, lhs_item.clone(), rhs_item.clone());
                            all_equal = ExprType::BinOp(
                                ExprBinOp::LogAnd,
                                all_equal.builtin(),
                                this_equal.builtin(),
                            );
                        }
                        Ok(all_equal)
                    }
                }

                match (&lhs_r.tok, op, &rhs_r.tok) {
                    (ExprType::Null, ExprBinOp::Eq, ExprType::Null) => {
                        Ok(ExprType::Value(T::new_from_bool(true)))
                    }
                    (ExprType::Null, ExprBinOp::Neq, ExprType::Null) => {
                        Ok(ExprType::Value(T::new_from_bool(false)))
                    }
                    (ExprType::Null, ExprBinOp::Eq, _) | (_, ExprBinOp::Eq, ExprType::Null) => {
                        Ok(ExprType::Value(T::new_from_bool(false)))
                    }
                    (ExprType::Null, ExprBinOp::Neq, _) | (_, ExprBinOp::Neq, ExprType::Null) => {
                        Ok(ExprType::Value(T::new_from_bool(true)))
                    }
                    (ExprType::Value(lhs_val), op, ExprType::Value(rhs_val)) => {
                        let value = match op {
                            ExprBinOp::Add => T::op_add(lhs_val, rhs_val),
                            ExprBinOp::Sub => T::op_sub(lhs_val, rhs_val),
                            ExprBinOp::Mult => T::op_mult(lhs_val, rhs_val),
                            ExprBinOp::Div => T::op_div(lhs_val, rhs_val),
                            ExprBinOp::Lt => T::op_lt(lhs_val, rhs_val),
                            ExprBinOp::Le => T::op_le(lhs_val, rhs_val),
                            ExprBinOp::Gt => T::op_gt(lhs_val, rhs_val),
                            ExprBinOp::Ge => T::op_ge(lhs_val, rhs_val),
                            ExprBinOp::Eq => T::op_eq(lhs_val, rhs_val),
                            ExprBinOp::Neq => T::op_neq(lhs_val, rhs_val),
                            _ => return Err(unsupported()),
                        };
                        Ok(ExprType::Value(value.map_err(at_op)?))
                    }
                    (ExprType::Object(lhs_obj), op, ExprType::Object(rhs_obj)) => match op {
                        ExprBinOp::Update => {
                            let mut merged = lhs_obj.clone();
                            for (k, v) in rhs_obj.iter() {
                                merged.insert(*k, v.clone());
                            }
                            Ok(ExprType::Object(merged))
                        }
                        ExprBinOp::Eq => {
                            let lhs_keys: HashSet<&StrKey> = lhs_obj.keys().collect();
                            let rhs_keys: HashSet<&StrKey> = rhs_obj.keys().collect();

                            if lhs_keys != rhs_keys {
                                Ok(ExprType::Value(T::new_from_bool(false)))
                            } else {
                                // After key-set equality, both BTreeMap iterators walk keys in
                                // the same sorted order, so values line up by matching key.
                                let lhs_vals =
                                    ExprType::List(lhs_obj.values().cloned().collect::<Vec<_>>());
                                let rhs_vals =
                                    ExprType::List(rhs_obj.values().cloned().collect::<Vec<_>>());
                                Ok(ExprType::BinOp(
                                    ExprBinOp::Eq,
                                    lhs_vals.builtin(),
                                    rhs_vals.builtin(),
                                ))
                            }
                        }
                        _ => Err(unsupported()),
                    },
                    (ExprType::List(lhs_list), op, ExprType::List(rhs_list)) => match op {
                        ExprBinOp::ListConcat => Ok(ExprType::List(
                            lhs_list
                                .iter()
                                .chain(rhs_list.iter())
                                .cloned()
                                .collect::<Vec<_>>(),
                        )),
                        ExprBinOp::Eq => list_eq(lhs_list, rhs_list),
                        ExprBinOp::Neq => list_eq(lhs_list, rhs_list)
                            .map(|eq| ExprType::UnOp(ExprUnOp::Not, eq.builtin())),
                        _ => Err(unsupported()),
                    },
                    (ExprType::Object(lhs_obj), ExprBinOp::HasAttr, ExprType::Value(rhs_val)) => {
                        let name = rhs_val.as_string().map_err(at_op)?;
                        Ok(ExprType::Value(T::new_from_bool(
                            lhs_obj.contains_key(&StrKey::from(&name)),
                        )))
                    }
                    (ExprType::Tuple(lhs_list), op, ExprType::Tuple(rhs_list)) => match op {
                        ExprBinOp::Eq => list_eq(lhs_list, rhs_list),
                        ExprBinOp::Neq => list_eq(lhs_list, rhs_list)
                            .map(|eq| ExprType::UnOp(ExprUnOp::Not, eq.builtin())),
                        _ => Err(unsupported()),
                    },
                    _ => Err(unsupported()),
                }
            }
        }
    }

    /// Whether an expression of this type still needs to be resolved, or is
    /// already in a resolved form.
    fn needs_resolve(tok: &ExprType<T, F>) -> bool {
        match tok {
            ExprType::Object(..) => false,
            ExprType::List(..) => false,
            ExprType::Tuple(..) => false,
            ExprType::Concat(..) => true,
            ExprType::AttrSel(..) => true,
            ExprType::Value(..) => false,
            ExprType::Var(..) => true,
            ExprType::UnOp(..) => true,
            ExprType::BinOp(..) => true,
            ExprType::FuncDef(..) => false,
            ExprType::FuncDefBuiltin(..) => false,
            ExprType::Let(..) => true,
            ExprType::Fold { .. } => true,
            ExprType::Map(..) => true,
            ExprType::FuncCall { .. } => true,
            ExprType::Bind(..) => true,
            ExprType::Switch(..) => true,
            ExprType::Match(..) => true,
            ExprType::Null => false,
            ExprType::UnderEval => true, // Never resolved, caught in resolve()
        }
    }

    pub fn resolve(&self) -> Result<(), F> {
        let storref: ExprStorage<T, F> = {
            let inner = self.inner_ref();
            match &inner.tok {
                // The cell is already being resolved further up the stack, so
                // its value depends on itself.
                ExprType::UnderEval => {
                    return Err(Error::new(
                        ErrorType::Eval,
                        "infinite recursion: the value depends on itself",
                    )
                    .reref(&inner.loc));
                }
                tok if !Self::needs_resolve(tok) => return Ok(()),
                _ => inner.clone(),
            }
        };

        // Blackholing: mark the cell as under evaluation while it is being
        // resolved, so that a cycle back into it is reported as an error
        // instead of recursing until the stack overflows. The original
        // expression is put back on error, so that a later evaluation (eval()
        // collecting all errors, or partial evaluation in the language server)
        // sees the unresolved expression again, as before.
        let original = storref.clone();
        self.0.as_ref().replace(ExprStorage {
            tok: ExprType::UnderEval,
            loc: storref.loc.clone(),
        });

        match Self::resolve_steps(storref) {
            Ok(resolved) => {
                self.0.as_ref().replace(resolved);
                Ok(())
            }
            Err(err) => {
                self.0.as_ref().replace(original);
                Err(err)
            }
        }
    }

    /// Rewrite an expression step by step until it is in a resolved form.
    ///
    /// A self-call in tail position is rewritten in place here, step after
    /// step, instead of nesting resolves, so an infinite one never reaches
    /// the blackholing check in [`Expr::resolve`]. It is caught by a budget
    /// on the number of steps for one cell instead: no real description
    /// rewrites a single expression anywhere near that many times.
    fn resolve_steps(mut storref: ExprStorage<T, F>) -> Result<ExprStorage<T, F>, F> {
        let mut first_match = true;
        let mut steps: usize = 0;

        while Self::needs_resolve(&storref.tok) {
            steps += 1;
            if steps > MAX_RESOLVE_STEPS {
                return Err(Error::new(
                    ErrorType::Eval,
                    format!(
                        "infinite recursion: the expression is still not resolved after {} steps",
                        MAX_RESOLVE_STEPS
                    ),
                )
                .reref(&storref.loc));
            }
            trace!(
                "{} {} at {}",
                if first_match {
                    "Resolving"
                } else {
                    "   ...   "
                },
                storref.tok.get_message().unwrap(),
                storref
                    .loc
                    .as_ref()
                    .map_or_else(|| "unknown location".to_string(), |loc| loc.to_string())
            );
            first_match = false;

            storref = match storref {
                ExprStorage {
                    tok: ExprType::Bind(varspace, bound_expr),
                    loc,
                } => match &*bound_expr.inner_ref() {
                    ExprStorage {
                        tok: ExprType::Object(fields),
                        ..
                    } => Ok(ExprType::Object(
                        fields
                            .iter()
                            .map(|(k, val)| (*k, val.bind(&varspace)))
                            .collect(),
                    )
                    .loc(loc)),
                    ExprStorage {
                        tok: ExprType::List(items),
                        ..
                    } => Ok(ExprType::List(
                        items.iter().map(|item| item.bind(&varspace)).collect(),
                    )
                    .loc(loc)),
                    ExprStorage {
                        tok: ExprType::Tuple(items),
                        ..
                    } => Ok(ExprType::Tuple(
                        items.iter().map(|item| item.bind(&varspace)).collect(),
                    )
                    .loc(loc)),
                    ExprStorage {
                        tok: ExprType::Concat(parts),
                        ..
                    } => Ok(ExprType::Concat(
                        parts.iter().map(|part| part.bind(&varspace)).collect(),
                    )
                    .loc(loc)),
                    ExprStorage {
                        tok: ExprType::AttrSel(val, attr),
                        ..
                    } => Ok(ExprType::AttrSel(val.bind(&varspace), attr.bind(&varspace)).loc(loc)),
                    ExprStorage {
                        tok: ExprType::Let(fields, target_expr),
                        ..
                    } => {
                        let mut vars: ExprSet<T, F> = varspace;
                        for (field_matcher, field_expr) in fields {
                            for (name, value) in field_matcher
                                .run(field_expr.bind(&vars))
                                .map_err(|e| e.reref(&loc))?
                                .into_iter()
                            {
                                vars.insert(name, value).map_or_else(
                                    || Ok(()),
                                    |_| {
                                        Err(Error::new(ErrorType::DupKey, name.as_string())
                                            .reref(&loc))
                                    },
                                )?;
                            }
                        }
                        Ok(target_expr.bind_fresh(&vars).inner_ref().clone())
                    }
                    ExprStorage {
                        tok: ExprType::FuncDef(matcher, func_expr),
                        ..
                    } => {
                        // Note: varspace move into the FuncDef here, but
                        // variables coming from the matcher needs higher
                        // priority. This all depends on when resolving FuncDef
                        // later in FuncCall, the resuling varspace is merged
                        // into the contained Bind
                        Ok(ExprType::FuncDef(
                            matcher.bind_defaults(&varspace),
                            // Unpacked again when the function is called
                            func_expr.bind_fresh(&varspace),
                        )
                        .loc(loc))
                    }
                    ExprStorage {
                        tok: ExprType::FuncDefBuiltin(expr_builtin),
                        loc: biloc,
                    } => Ok(ExprType::FuncDefBuiltin(expr_builtin.clone()).loc(biloc.clone())),
                    ExprStorage {
                        tok: ExprType::Fold { func, init, input },
                        ..
                    } => Ok(ExprType::Fold {
                        func: func.bind(&varspace),
                        // Taken over as the result when the list is empty
                        init: init.as_ref().map(|e| e.bind_fresh(&varspace)),
                        input: input.bind(&varspace),
                    }
                    .loc(loc)),
                    ExprStorage {
                        tok: ExprType::Map(typ, func, input, filter),
                        ..
                    } => Ok(ExprType::Map(
                        *typ,
                        func.bind(&varspace),
                        input.bind(&varspace),
                        filter.as_ref().map(|e| e.bind(&varspace)),
                    )
                    .loc(loc)),
                    ExprStorage {
                        tok: ExprType::Var(name),
                        loc: vloc,
                    } => match &varspace.get(name) {
                        Some(value) => {
                            let resolved_loc = value.get_loc();
                            Ok(value
                                .res_type()
                                .map_err(|e| e.reref(&loc))?
                                .tok
                                .clone()
                                .loc(Loc::chain(loc, resolved_loc)))
                        }
                        None => {
                            let err = Error::new(
                                ErrorType::Scope,
                                format!("unknown variable `{}`", name),
                            )
                            .reref(vloc);
                            let known = varspace.keys().map(|key| key.as_string());
                            Err(match closest(&name.as_string(), known) {
                                Some(similar) => err.help(format!(
                                    "a variable with a similar name exists: `{similar}`"
                                )),
                                None => err,
                            })
                        }
                    },
                    ExprStorage {
                        tok: ExprType::UnOp(op, expr),
                        ..
                    } => Ok(ExprType::UnOp(*op, expr.bind(&varspace)).loc(loc)),
                    ExprStorage {
                        tok: ExprType::BinOp(op, lhs, rhs),
                        ..
                    } => {
                        Ok(ExprType::BinOp(*op, lhs.bind(&varspace), rhs.bind(&varspace)).loc(loc))
                    }
                    ExprStorage {
                        tok:
                            ExprType::FuncCall {
                                arg: fargs,
                                func: fexpr,
                            },
                        ..
                    } => Ok(ExprType::FuncCall {
                        arg: fargs.bind(&varspace),
                        func: fexpr.bind(&varspace),
                    }
                    .loc(loc)),
                    ExprStorage {
                        tok: ExprType::Value(value),
                        ..
                    } => Ok(ExprType::Value(value.clone()).loc(loc)),
                    ExprStorage {
                        tok: ExprType::Bind(inner_vars, inner_expr),
                        ..
                    } => Ok(inner_expr.bind_fresh(inner_vars).inner_ref().clone()),
                    ExprStorage {
                        tok: ExprType::Switch(inner_expr, inner_cases, inner_default),
                        ..
                    } => Ok(ExprType::Switch(
                        inner_expr.bind(&varspace),
                        inner_cases
                            .iter()
                            // The body of the case that matches is taken over
                            .map(|(m, e)| (m.bind(&varspace), e.bind_fresh(&varspace)))
                            .collect(),
                        inner_default.as_ref().map(|e| e.bind_fresh(&varspace)),
                    )
                    .loc(loc)),
                    ExprStorage {
                        tok: ExprType::Match(inner_expr, inner_cases),
                        ..
                    } => Ok(ExprType::Match(
                        inner_expr.bind(&varspace),
                        inner_cases
                            .iter()
                            // Unpacked again for the case that matches
                            .map(|(m, e)| (m.bind_defaults(&varspace), e.bind_fresh(&varspace)))
                            .collect(),
                    )
                    .loc(loc)),
                    ExprStorage {
                        tok: ExprType::Null,
                        ..
                    } => Ok(ExprType::Null.loc(loc)),
                    ExprStorage {
                        tok: ExprType::UnderEval,
                        ..
                    } => unreachable!("ExprType::UnderEval should not be in a Bind expression"),
                },
                ExprStorage {
                    tok: ExprType::Concat(parts),
                    loc,
                } => {
                    let part_values = parts
                        .iter()
                        .map(|part| part.value().map_err(|e| e.reref(&loc)))
                        .collect::<Result<Vec<T>, F>>()?;
                    Ok(
                        ExprType::Value(T::op_string_concat(part_values).map_err(|e| e.reref(&loc))?)
                            .loc(loc),
                    )
                }
                ExprStorage {
                    tok: ExprType::AttrSel(val, attr),
                    loc,
                } => {
                    // TODO: Don't need to resolve here...
                    // Need to resolve before clone here, so we guarantee that
                    // an expression in an object is only evaluated at most once.
                    // However, it may violate the laziness directive that if the
                    // variable is not used, it doesn't need to be evaluated.
                    //
                    // To resolve this, a better combined Rc<RefCell<...>> that can
                    // Merge two references is needed, to transfer a var resolution
                    // to the next object.
                    let attr_name = attr
                        .value()
                        .map_err(|e| e.reref(&loc))?
                        .as_string()
                        .map_err(|e| e.reref(&loc))?;
                    let value = val
                        .get_item(attr_name.as_str())
                        .map_err(|e| e.reref(&loc))?;
                    value.resolve().map_err(|e| e.reref(&loc))?;
                    let resolved_loc = value.get_loc();
                    Ok(value
                        .inner_ref()
                        .tok
                        .clone()
                        .loc(Loc::chain(loc, resolved_loc)))
                }
                ExprStorage {
                    tok:
                        ExprType::FuncCall {
                            arg: fargs,
                            func: fexpr,
                        },
                    loc,
                } => match &*fexpr.res_type().map_err(|e| e.reref(&loc))? {
                    ExprStorage {
                        tok: ExprType::FuncDef(matcher, fimpl),
                        ..
                    } => {
                        let (mut fbound, fexpr) = match &fimpl.inner_ref().tok {
                            ExprType::Bind(fbound, fexpr) => (fbound.clone(), fexpr.clone()),
                            _ => (ExprSet::new(), fimpl.clone()),
                        };

                        let mut vars = matcher.run(fargs).map_err(|e| e.reref(&loc))?;

                        fbound.append(&mut vars);

                        Ok(fexpr.bind_fresh(&fbound).inner_ref().clone())
                    }
                    ExprStorage {
                        tok: ExprType::FuncDefBuiltin(ExprBuiltinWrapper(_, funcrc)),
                        ..
                    } => {
                        // Resolve the returned cell in place and share its
                        // children, rather than copying it: a builtin may
                        // return a cell that is referenced from elsewhere
                        // (ops.recurse ties its output to the argument), and
                        // a copy would be evaluated again for every reference.
                        let result = funcrc.as_ref().call(fargs).map_err(|e| e.reref(&loc))?;
                        result.resolve().map_err(|e| e.reref(&loc))?;
                        let resolved_loc = result.get_loc();
                        Ok(result
                            .inner_ref()
                            .tok
                            .clone()
                            .loc(Loc::chain(loc, resolved_loc)))
                    }
                    ExprStorage { tok, .. } => Err(Error::new(
                        ErrorType::Type,
                        format!("cannot call {}, it is not a function", tok.describe()),
                    )
                    .primary(&loc, "")
                    .label(&fexpr.origin(), format!("this is {}", tok.describe()))),
                },
                ExprStorage {
                    tok: ExprType::Fold { func, init, input },
                    loc,
                } => {
                    let mut output = init;
                    match &*input.res_type().map_err(|e| e.reref(&loc))? {
                        ExprStorage {
                            tok: ExprType::List(input_items),
                            loc: input_loc,
                        } => {
                            for item in input_items.iter() {
                                item.resolve().map_err(|e| e.reref(&item.get_loc()))?;
                                output = match output {
                                    Some(prev) => Some(
                                        ExprType::FuncCall {
                                            arg: item.clone(),
                                            func: ExprType::FuncCall {
                                                arg: prev,
                                                func: func.clone(),
                                            }
                                            .reref(input_loc.clone()),
                                        }
                                        .reref(item.get_loc()),
                                    ),
                                    None => Some(item.clone()),
                                };
                            }
                            if let Some(output) = output {
                                Ok(output.inner_ref().clone())
                            } else {
                                Err(Error::new(
                                    ErrorType::Eval,
                                    "cannot fold an empty list without a start value",
                                )
                                .primary(&loc, "")
                                .label(&input.origin(), "the list is empty"))
                            }
                        }
                        other => Err(Error::new(
                            ErrorType::Type,
                            format!(
                                "cannot fold over {}, it is not a list",
                                other.tok.describe()
                            ),
                        )
                        .primary(&loc, "")
                        .label(&input.origin(), format!("this is {}", other.tok.describe()))),
                    }
                }
                ExprStorage {
                    tok: ExprType::Map(typ, func, input, filter),
                    loc,
                } => {
                    input.resolve().map_err(|e| e.reref(&loc))?;
                    let input_items: Vec<Expr<T, F>> = match &*input.inner_ref() {
                        ExprStorage {
                            tok: ExprType::List(input_vec),
                            ..
                        } => Ok(input_vec.to_vec()),
                        ExprStorage {
                            tok: ExprType::Object(args),
                            ..
                        } => {
                            let args = args
                                .iter()
                                .map(|(k, v)| {
                                    ExprType::Tuple(vec![
                                        ExprType::Value(T::new_from_string(k.as_string()))
                                            .reref(v.get_loc()),
                                        v.clone(),
                                    ])
                                    .reref(v.get_loc())
                                })
                                .collect::<Vec<_>>();
                            Ok(args)
                        }
                        other => Err(Error::new(
                            ErrorType::Type,
                            format!(
                                "cannot iterate over {}, it is not a list or an object",
                                other.tok.describe()
                            ),
                        )
                        .primary(&loc, "")
                        .label(&input.origin(), format!("this is {}", other.tok.describe()))),
                    }?;

                    let mut filtered_items: Vec<Expr<T, F>>;
                    if let Some(filter_expr) = filter {
                        filtered_items = Vec::new();
                        for item in input_items.iter() {
                            let filter_result = ExprType::FuncCall {
                                arg: item.clone(),
                                func: filter_expr.clone(),
                            }
                            .reref(item.get_loc())
                            .value()
                            .map_err(|e| e.reref(&item.get_loc()))?
                            .as_bool()
                            .map_err(|e| e.reref(&item.get_loc()))?;
                            if filter_result {
                                filtered_items.push(item.clone());
                            }
                        }
                    } else {
                        filtered_items = input_items;
                    };

                    let output_items = filtered_items
                        .into_iter()
                        .map(|iel| {
                            let loc = iel.get_loc();
                            ExprType::FuncCall {
                                arg: iel,
                                func: func.clone(),
                            }
                            .reref(loc)
                        })
                        .collect::<Vec<_>>();

                    let output = match typ {
                        ExprMapType::List => ExprType::List(output_items),
                        ExprMapType::Object => ExprType::Object(
                            output_items
                                .into_iter()
                                .map(|el| match &*el.res_type().map_err(|e| e.reref(&loc))? {
                                    ExprStorage {
                                        tok: ExprType::Tuple(els),
                                        ..
                                    } if els.len() == 2 => Ok((
                                        StrKey::from(
                                            &els[0]
                                                .value()
                                                .map_err(|e| e.reref(&el.get_loc()))?
                                                .as_string()
                                                .map_err(|e| e.reref(&el.get_loc()))?,
                                        ),
                                        els[1].clone(),
                                    )),
                                    other => Err(Error::new(
                                        ErrorType::Type,
                                        format!(
                                            "an object is built from tuples of a name and a \
                                             value, got {}",
                                            other.tok.describe()
                                        ),
                                    )
                                    .reref(&el.get_loc())),
                                })
                                .collect::<Result<ExprSet<T, F>, F>>()?,
                        ),
                    };
                    Ok(output.loc(loc))
                }
                ExprStorage {
                    tok: ExprType::UnOp(op, expr),
                    loc,
                } => {
                    expr.resolve().map_err(|e| e.reref(&loc))?;
                    // An error from the operation, pointed at it and at
                    // where the value was written
                    let at_op = |err: Error<F>| {
                        err.primary(&loc, "")
                            .label(&expr.origin(), format!("this is {}", expr.describe()))
                    };
                    let result = match &expr.inner_ref().tok {
                        ExprType::Value(value) => match op {
                            ExprUnOp::Neg => value.op_neg(),
                            ExprUnOp::Not => value.op_not(),
                        },
                        other => Err(Error::new(
                            ErrorType::Type,
                            format!("cannot apply `{}` to {}", op, other.describe()),
                        )),
                    };
                    Ok(ExprType::Value(result.map_err(at_op)?).loc(loc))
                }
                ExprStorage {
                    tok: ExprType::BinOp(op, lhs, rhs),
                    loc,
                } => Ok(Self::resolve_binop(&lhs, op, &rhs, &loc)?.loc(loc)),
                ExprStorage {
                    tok: ExprType::Switch(ref_expr, cases, default_case),
                    loc,
                } => {
                    let outcome = cases
                        .iter()
                        .map(|(matcher, outcome)| {
                            let compare =
                                ExprType::BinOp(ExprBinOp::Eq, matcher.clone(), ref_expr.clone())
                                    .reref(matcher.get_loc());
                            if let Some(is_match) = compare
                                .res_type()
                                .map_err(|e| e.reref(&loc))?
                                .tok
                                .try_as_value_ref()
                            {
                                let found = is_match.as_bool().map_err(|e| e.reref(&loc))?;
                                if found {
                                    Ok(Some(outcome.clone()))
                                } else {
                                    Ok(None)
                                }
                            } else {
                                Ok(None)
                            }
                        })
                        .collect::<Result<Vec<Option<Expr<T, F>>>, F>>()?
                        .into_iter()
                        .flatten()
                        .next();

                    if let Some(outcome_expr) = outcome {
                        Ok(outcome_expr.inner_ref().tok.clone().loc(loc))
                    } else {
                        if let Some(default_expr) = default_case {
                            Ok(default_expr.inner_ref().tok.clone().loc(loc))
                        } else {
                            Err(Error::new(
                                ErrorType::Eval,
                                format!("no case matches the value {}", ref_expr.diag()),
                            )
                            .primary(&loc, "")
                            .label(
                                &ref_expr.origin(),
                                format!("this is {}", ref_expr.describe()),
                            ))
                        }
                    }
                }
                ExprStorage {
                    tok: ExprType::Match(ref_expr, cases),
                    loc,
                } => {
                    let mut outcome = None;
                    for (matcher, case_expr) in cases.iter() {
                        if let Some(vars) = matcher
                            .try_run(ref_expr.clone())
                            .map_err(|e| e.reref(&loc))?
                        {
                            outcome = Some((vars, case_expr));
                            break;
                        }
                    }

                    if let Some((mut vars, case_expr)) = outcome {
                        // Same as a FuncCall: the names bound by the matcher
                        // take priority over the scope the case was bound in
                        let (mut bound, body) = match &case_expr.inner_ref().tok {
                            ExprType::Bind(bound, body) => (bound.clone(), body.clone()),
                            _ => (ExprSet::new(), case_expr.clone()),
                        };
                        bound.append(&mut vars);
                        Ok(body.bind_fresh(&bound).inner_ref().clone())
                    } else {
                        Err(Error::new(
                            ErrorType::Eval,
                            format!("no case matches the value {}", ref_expr.diag()),
                        )
                        .primary(&loc, "")
                        .label(
                            &ref_expr.origin(),
                            format!("this is {}", ref_expr.describe()),
                        ))
                    }
                }
                ExprStorage {
                    tok: ExprType::Var(name),
                    loc,
                } => Err(
                    Error::new(ErrorType::Scope, format!("unknown variable `{}`", name))
                        .reref(&loc),
                ),
                ExprStorage {
                    tok: ExprType::UnderEval,
                    loc: _,
                } => unreachable!("UnderEval is only stored in a cell, never resolved as a step"),
                ExprStorage { tok, loc: _ } => unreachable!("Resolving {}", tok),
            }?;
        }

        Ok(storref)
    }

    pub fn replace_storage(&self, storref: ExprStorage<T, F>) {
        self.0.as_ref().replace(storref);
    }

    fn res_type(&self) -> Result<Ref<'_, ExprStorage<T, F>>, F> {
        self.resolve()?;
        Ok(self.inner_ref())
    }

    /// Resolve the expression and everything in it, and report the first
    /// error. The rest of a list or object is still evaluated after an
    /// error in one of its parts.
    pub fn eval(&self) -> Result<(), F> {
        // A cell that fails to resolve keeps its unresolved expression, and
        // its parts are not evaluated: the scope of a bound expression holds
        // the `self` of its module, which leads straight back to this cell.
        self.resolve()?;

        // Each part with its place in the result, for an error in it
        let fields: Vec<(String, Expr<T, F>)> = match &self.inner_ref().tok {
            ExprType::Object(fields) => fields
                .iter()
                .map(|(name, field)| (format!(".{name}"), field.clone()))
                .collect(),
            ExprType::List(items) | ExprType::Tuple(items) => items
                .iter()
                .enumerate()
                .map(|(index, item)| (format!("[{index}]"), item.clone()))
                .collect(),
            _ => vec![],
        };

        let mut first_err: Option<Error<F>> = None;
        for (part, ex) in fields.into_iter() {
            if let Err(err) = ex.eval()
                && first_err.is_none()
            {
                first_err = Some(err.in_result(part));
            }
        }

        match first_err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    pub fn value(&self) -> Result<T, F> {
        // Since we expect a string, we only need to resolve one level.
        self.resolve()?;
        match &self.inner_ref().tok {
            ExprType::Value(val) => Ok(val.clone()),
            other => Err(Error::new(
                ErrorType::NoValue,
                format!("expected a plain value, got {}", other.describe()),
            )),
        }
    }

    pub fn eval_string(&self) -> Result<String, F> {
        // Since we expect a string, we only need to resolve one level.
        self.resolve()?;
        match &self.inner_ref().tok {
            ExprType::Value(val) => Ok(val.as_string()?),
            other => Err(Error::new(
                ErrorType::NoValue,
                format!("expected a string, got {}", other.describe()),
            )),
        }
    }

    pub fn get_item(&self, name: &str) -> Result<Expr<T, F>, F> {
        self.resolve()?;
        let node = self.inner_ref();
        match &node.tok {
            ExprType::Object(vars) => match vars.get(&StrKey::from(name)) {
                Some(value) => Ok(value.clone()),
                None => {
                    let err = Error::new(
                        ErrorType::NoValue,
                        format!("the object has no field `{name}`"),
                    )
                    .label(&self.origin(), "this is the object");
                    let fields = || vars.keys().map(|key| key.as_string());
                    Err(if let Some(similar) = closest(name, fields()) {
                        err.help(format!("a field with a similar name exists: `{similar}`"))
                    } else if vars.is_empty() {
                        err.help("the object has no fields at all")
                    } else if vars.len() <= MAX_LISTED_FIELDS {
                        err.help(format!("the object has the fields {}", name_list(fields())))
                    } else {
                        err
                    })
                }
            },
            other => Err(Error::new(
                ErrorType::Type,
                format!(
                    "cannot select the field `{name}` of {}, it is not an object",
                    other.describe()
                ),
            )
            .label(&self.origin(), format!("this is {}", other.describe()))),
        }
    }

    pub fn new_builtin(func: impl ExprBuiltin<T, F> + 'static) -> Expr<T, F> {
        ExprType::new_builtin(func).builtin()
    }

    pub fn from_builtins(value: Vec<Rc<dyn ExprBuiltin<T, F>>>) -> Expr<T, F> {
        let mut exprset = ExprSet::new();

        for bi in value.into_iter() {
            let name = bi.get_name();
            exprset
                .insert(
                    name,
                    ExprType::FuncDefBuiltin(ExprBuiltinWrapper(name, bi)).builtin(),
                )
                .unwrap();
        }

        ExprType::Object(exprset).builtin()
    }
}
