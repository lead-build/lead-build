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

/// A cell being resolved by [`Expr::resolve`].
struct Frame<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    cell: Expr<T, F>,
    /// The expression so far, rewritten step by step
    storage: ExprStorage<T, F>,
    /// What the cell held before, put back if the frame fails
    original: ExprStorage<T, F>,
    /// Steps taken, for the budget against a loop in tail position
    steps: usize,
    /// Where a step that forces the items of a list one by one got to, so it
    /// does not check them all again each time it runs. Reset for each new
    /// storage.
    cursor: usize,
    /// Added to the trace of an error from this frame, in this order. Set
    /// by whoever pushed the frame, as the locations the native resolve of
    /// that child added.
    trace: Trace<F>,
    /// What the frame was needed for, said of the first location of `trace`
    role: Option<Role<T, F>>,
}

/// Locations added to the trace of an error from a frame, see
/// [`Frame::trace`]. Two at most: a child that a step resolves through a
/// comparison it builds (`switch`) passes through that comparison too.
type Trace<F> = [Option<Loc<F>>; 2];

/// A child that must be resolved before a step can go on, see `force!` in
/// [`Expr::step`].
struct Pending<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    child: Expr<T, F>,
    /// The locations to add to the trace if the child fails
    trace: Trace<F>,
    /// What the step needs the child for, said of the first location
    role: Role<T, F>,
}

/// What a step needs a child for, for the trace of an error. Only turned into
/// text when an error is reported, so that asking for a child costs nothing.
enum Role<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    Variable(StrKey),
    /// With the expression of the name, which is resolved by then
    Field(Expr<T, F>),
    ObjectOfField(Expr<T, F>),
    FieldName,
    Function,
    Operand(ExprUnOp),
    LeftSide(ExprBinOp),
    RightSide(ExprBinOp),
    StringPart,
    ForInput,
    FoldInput,
    FoldItem(usize),
    SwitchCase,
    SwitchValue,
    MatchValue,
}

impl<T, F> Role<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    fn describe(&self) -> String {
        let field = |name: &Expr<T, F>| match &name.0.as_ref().borrow().tok {
            ExprType::Value(value) => value
                .as_string()
                .map_or_else(|_| "a field".to_string(), |name| format!("field `{name}`")),
            _ => "a field".to_string(),
        };
        match self {
            Role::Variable(name) => format!("variable `{name}`"),
            Role::Field(name) => field(name),
            Role::ObjectOfField(name) => format!("object of {}", field(name)),
            Role::FieldName => "name of a field".to_string(),
            Role::Function => "function of a call".to_string(),
            Role::Operand(op) => format!("operand of `{op}`"),
            Role::LeftSide(op) => format!("left side of `{op}`"),
            Role::RightSide(op) => format!("right side of `{op}`"),
            Role::StringPart => "part of a string".to_string(),
            Role::ForInput => "input of a `for`".to_string(),
            Role::FoldInput => "input of a fold".to_string(),
            Role::FoldItem(index) => format!("item [{index}] of a fold"),
            Role::SwitchCase => "case of a `switch`".to_string(),
            Role::SwitchValue => "value of a `switch`".to_string(),
            Role::MatchValue => "value of a `match`".to_string(),
        }
    }
}

/// The outcome of one call of [`Expr::step`].
enum Step<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    /// The expression rewritten, which may need more steps
    Next(ExprStorage<T, F>),
    /// The expression unchanged, as it needs this child resolved first
    Need(Pending<T, F>, ExprStorage<T, F>),
}

/// The name under which a scope holds the call it was made for, see
/// [`Expr::bind_fresh`]. It is not a valid identifier, so no variable can
/// have it.
fn call_key() -> StrKey {
    static KEY: std::sync::OnceLock<StrKey> = std::sync::OnceLock::new();
    *KEY.get_or_init(|| StrKey::from("<call>"))
}

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
    ///
    /// The location of the new cell gets the call the scope was made for
    /// (see [`Expr::call_marker`]), so that an error in it can show where
    /// the function it is part of was called from.
    pub fn bind_fresh(&self, varspace: &ExprSet<T, F>) -> Expr<T, F> {
        let mut loc = self.get_loc();
        if let Some(loc) = &mut loc
            && let Some(marker) = varspace.get(&call_key())
            && let Some(call) = marker.inner_ref().loc.as_ref().and_then(|l| l.ctx.clone())
        {
            loc.ctx = Some(call);
        }
        ExprType::Bind(varspace.clone(), self.clone()).reref(loc)
    }

    /// The entry a function call adds to the scope of the function body,
    /// under [`call_key`]: a cell that only carries the location of the
    /// call, as the `ctx` of its own location, ready to be shared by every
    /// cell bound in that scope.
    fn call_marker(call_site: &Loc<F>) -> Expr<T, F> {
        let call = match &call_site.ctx {
            // A recursion calls from the same expression round after round.
            // One entry stands for all of those rounds, as they would be
            // shown as one anyway, and the chain does not grow with the
            // depth of the recursion. (Spans are compared without their
            // files; a call that calls into another file from the very same
            // span is not worth a bound on the file type.)
            Some(outer) if outer.span == call_site.span => outer.clone(),
            _ => Rc::new(call_site.clone()),
        };
        let mut loc = Loc::new(call_site.file.clone(), call_site.span.clone());
        loc.ctx = Some(call);
        ExprType::Null.reref(Some(loc))
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

    /// Resolve the expression in place, if it is not resolved already.
    ///
    /// The evaluation does not recurse on the native stack for the children
    /// an expression needs: a step that needs a child ends there and asks
    /// for it (`force!` in [`Expr::step`]), the child is resolved on a stack
    /// of frames on the heap, and the step runs again. So the depth of an
    /// evaluation is limited by memory, not by the stack of the thread. A
    /// builtin or a pattern that resolves a value itself still nests a new
    /// resolve.
    pub fn resolve(&self) -> Result<(), F> {
        let mut frames: Vec<Frame<T, F>> = match Self::enter(self, [None, None], None)? {
            Some(frame) => vec![frame],
            None => return Ok(()),
        };

        while let Some(top) = frames.last_mut() {
            if !Self::needs_resolve(&top.storage.tok) {
                let Some(done) = frames.pop() else {
                    unreachable!("the loop runs while there is a frame")
                };
                done.cell.0.as_ref().replace(done.storage);
                continue;
            }

            // Only steps that rewrite count, not ones that ask for a child:
            // those are bounded by the number of children
            if top.steps >= MAX_RESOLVE_STEPS {
                let err = Error::new(
                    ErrorType::Eval,
                    format!(
                        "infinite recursion: the expression is still not resolved after {} steps",
                        MAX_RESOLVE_STEPS
                    ),
                )
                .reref(&top.storage.loc);
                return Err(Self::unwind(frames, err));
            }
            trace!(
                "{} {} at {}",
                if top.steps == 0 { "Resolving" } else { "   ...   " },
                top.storage.tok.get_message().unwrap(),
                top.storage
                    .loc
                    .as_ref()
                    .map_or_else(|| "unknown location".to_string(), |loc| loc.to_string())
            );

            let storage = std::mem::take(&mut top.storage);
            match Self::step(storage, &mut top.cursor) {
                Ok(Step::Next(next)) => {
                    top.storage = next;
                    top.steps += 1;
                    top.cursor = 0;
                }
                // The child is resolved as a frame of its own, and the step
                // runs again after it
                Ok(Step::Need(Pending { child, trace, role }, unchanged)) => {
                    top.storage = unchanged;
                    match Self::enter(&child, trace, Some(role)) {
                        Ok(Some(frame)) => frames.push(frame),
                        Ok(None) => unreachable!("`force!` only asks for an unresolved child"),
                        Err(err) => return Err(Self::unwind(frames, err)),
                    }
                }
                Err(err) => return Err(Self::unwind(frames, err)),
            }
        }
        Ok(())
    }

    /// Start resolving `cell`: `None` if it is resolved already, otherwise
    /// a frame for it, with the cell marked as under evaluation.
    ///
    /// The mark is the blackholing that detects a cycle: a value that needs
    /// itself finds its own cell marked and fails instead of recursing until
    /// memory runs out. The original expression is put back if the frame
    /// fails, see [`Expr::unwind`].
    fn enter(
        cell: &Expr<T, F>,
        trace: Trace<F>,
        role: Option<Role<T, F>>,
    ) -> Result<Option<Frame<T, F>>, F> {
        let storage: ExprStorage<T, F> = {
            let inner = cell.inner_ref();
            match &inner.tok {
                // The cell is already being resolved further up, so its value
                // depends on itself.
                ExprType::UnderEval => {
                    let err = Error::new(
                        ErrorType::Eval,
                        "infinite recursion: the value depends on itself",
                    )
                    .reref(&inner.loc);
                    return Err(Self::add_trace(err, &trace, &role));
                }
                tok if !Self::needs_resolve(tok) => return Ok(None),
                _ => inner.clone(),
            }
        };
        cell.0.as_ref().replace(ExprStorage {
            tok: ExprType::UnderEval,
            loc: storage.loc.clone(),
        });
        Ok(Some(Frame {
            cell: cell.clone(),
            original: storage.clone(),
            storage,
            steps: 0,
            cursor: 0,
            trace,
            role,
        }))
    }

    /// Fail every frame with `err`, innermost first: each cell gets its
    /// original expression back, so that a later evaluation sees it
    /// unresolved as before, and each frame adds to the trace what the step
    /// that needed it would have added.
    fn unwind(frames: Vec<Frame<T, F>>, mut err: Error<F>) -> Error<F> {
        for frame in frames.into_iter().rev() {
            frame.cell.0.as_ref().replace(frame.original);
            err = Self::add_trace(err, &frame.trace, &frame.role);
        }
        err
    }

    /// Whether [`Expr::resolve_binop`] resolves the right side, given the
    /// resolved left side.
    fn binop_needs_rhs(op: ExprBinOp, lhs: &Expr<T, F>) -> bool {
        let lhs_true = || match &lhs.inner_ref().tok {
            ExprType::Value(value) => value.as_bool().ok(),
            _ => None,
        };
        match op {
            // Without a boolean on the left, it fails before the right side
            ExprBinOp::LogAnd | ExprBinOp::LogImpl => lhs_true() == Some(true),
            ExprBinOp::LogOr => lhs_true() == Some(false),
            _ => true,
        }
    }

    /// Add a failed frame's locations to the trace of `err`.
    fn add_trace(err: Error<F>, trace: &Trace<F>, role: &Option<Role<T, F>>) -> Error<F> {
        let [first, second] = trace;
        let err = match role {
            Some(role) => err.reref_as(first, role.describe()),
            None => err.reref(first),
        };
        err.reref(second)
    }

    /// Item `index` of `list`, if it is a list that long.
    fn list_item(list: &Expr<T, F>, index: usize) -> Option<Expr<T, F>> {
        match &list.inner_ref().tok {
            ExprType::List(items) => items.get(index).cloned(),
            _ => None,
        }
    }

    /// Whether `expr` still needs resolving, for `force!` in [`Expr::step`].
    fn unresolved(expr: &Expr<T, F>) -> bool {
        Self::needs_resolve(&expr.inner_ref().tok)
    }

    /// Whether matching `pattern` resolves the value, or only names it.
    /// Deeper parts of the pattern resolve their parts natively.
    fn pattern_resolves(pattern: &Matcher<T, F>) -> bool {
        match pattern {
            Matcher::Alias(inner, _) => Self::pattern_resolves(inner),
            Matcher::Tuple(..) | Matcher::List(..) | Matcher::Object(..) => true,
            _ => false,
        }
    }

    /// One rewrite step of an expression that is not resolved yet: the
    /// expression it turns into, which may need more steps.
    ///
    /// A self-call in tail position is rewritten in place, step after step,
    /// instead of nesting resolves, so an infinite one never reaches the
    /// blackholing check in [`Expr::enter`]. It is caught by a budget on the
    /// number of steps for one cell instead, in [`Expr::resolve`]: no real
    /// description rewrites a single expression anywhere near that many
    /// times.
    ///
    /// A step that needs a child resolved uses `force!` on it first, right
    /// before the code that uses it. An unresolved child ends the step: it is
    /// resolved by [`Expr::resolve`] as a frame of its own, and the step runs
    /// again from the start, now finding it resolved. Only a child the step
    /// uses unconditionally may be forced, and in the order it is used, or
    /// what is evaluated changes. The arm's own `resolve()`/`value()` calls
    /// then return at once; a child that is not forced is still resolved by
    /// them, natively.
    fn step(storref: ExprStorage<T, F>, cursor: &mut usize) -> Result<Step<T, F>, F> {
        // Resolve `$child` before going on. If it is not resolved, the step
        // ends here and hands back `$storage`, the unchanged expression
        // rebuilt from the arm's bindings. `$trace` is what the child's
        // failure adds to the trace, the same as the arm's `map_err` on the
        // line that uses the child, and `$role` says what the child is for.
        // The arguments are only evaluated when the child is unresolved.
        macro_rules! force {
            ($child:expr, $role:expr, $trace:expr, $storage:expr) => {
                if Self::unresolved(&$child) {
                    return Ok(Step::Need(
                        Pending {
                            child: Expr::clone(&$child),
                            trace: $trace,
                            role: $role,
                        },
                        $storage,
                    ));
                }
            };
        }

        let next = match storref {
            ExprStorage {
                tok: ExprType::Bind(varspace, bound_expr),
                loc,
            } => {
                // A variable: the cell it refers to, as the arm for it below
                // takes its value
                let var = match &bound_expr.inner_ref().tok {
                    ExprType::Var(name) => Some(*name),
                    _ => None,
                };
                if let Some(name) = var
                    && let Some(value) = varspace.get(&name)
                {
                    force!(
                        value,
                        Role::Variable(name),
                        [loc.clone(), None],
                        ExprStorage {
                            tok: ExprType::Bind(varspace, bound_expr),
                            loc
                        }
                    );
                }
                match &*bound_expr.inner_ref() {
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
                            // The location of the bound cell, which knows
                            // the call it is in; the variable itself is the
                            // expression as written, shared by all calls
                            let err = Error::new(
                                ErrorType::Scope,
                                format!("unknown variable `{}`", name),
                            )
                            .reref(if loc.is_some() { &loc } else { vloc });
                            let known = varspace
                                .keys()
                                .filter(|key| **key != call_key())
                                .map(|key| key.as_string());
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
                }
            }
            ExprStorage {
                tok: ExprType::Concat(parts),
                loc,
            } => {
                while let Some(part) = parts.get(*cursor).cloned() {
                    force!(
                        part,
                        Role::StringPart,
                        [loc.clone(), None],
                        ExprStorage {
                            tok: ExprType::Concat(parts),
                            loc
                        }
                    );
                    *cursor += 1;
                }
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
                //
                // The name, the object, then the field
                force!(attr, Role::FieldName, [loc.clone(), None], ExprStorage {
                        tok: ExprType::AttrSel(val, attr),
                        loc
                    });
                force!(val, Role::ObjectOfField(attr.clone()), [loc.clone(), None], ExprStorage {
                        tok: ExprType::AttrSel(val, attr),
                        loc
                    });
                let attr_name = attr
                    .value()
                    .map_err(|e| e.reref(&loc))?
                    .as_string()
                    .map_err(|e| e.reref(&loc))?;
                let value = val
                    .get_item(attr_name.as_str())
                    .map_err(|e| e.reref(&loc))?;
                force!(value, Role::Field(attr.clone()), [loc.clone(), None], ExprStorage {
                        tok: ExprType::AttrSel(val, attr),
                        loc
                    });
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
            } => {
                // The function; the argument is left to its pattern
                force!(
                    fexpr,
                    Role::Function,
                    [loc.clone(), None],
                    ExprStorage {
                        tok: ExprType::FuncCall {
                            arg: fargs,
                            func: fexpr
                        },
                        loc
                    }
                );
                match &*fexpr.res_type().map_err(|e| e.reref(&loc))? {
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
                        if let Some(call_site) = &loc {
                            fbound.insert(call_key(), Self::call_marker(call_site));
                        }

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
                }
            }
            ExprStorage {
                tok: ExprType::Fold { func, init, input },
                loc,
            } => {
                // The list, then each item in turn
                force!(input, Role::FoldInput, [loc.clone(), None], ExprStorage {
                        tok: ExprType::Fold { func, init, input },
                        loc
                    });
                while let Some(item) = Self::list_item(&input, *cursor) {
                    force!(item, Role::FoldItem(*cursor), [item.get_loc(), None], ExprStorage {
                        tok: ExprType::Fold { func, init, input },
                        loc
                    });
                    *cursor += 1;
                }
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
                // The list or object; the rest are cells this step creates
                force!(
                    input,
                    Role::ForInput,
                    [loc.clone(), None],
                    ExprStorage {
                        tok: ExprType::Map(typ, func, input, filter),
                        loc
                    }
                );
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
                force!(
                    expr,
                    Role::Operand(op),
                    [loc.clone(), None],
                    ExprStorage {
                        tok: ExprType::UnOp(op, expr),
                        loc
                    }
                );
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
            } => {
                // As `resolve_binop` uses them: the left side, and the right
                // side unless the left one decides a logical operator
                force!(lhs, Role::LeftSide(op), [lhs.get_loc(), None], ExprStorage {
                        tok: ExprType::BinOp(op, lhs, rhs),
                        loc
                    });
                if Self::binop_needs_rhs(op, &lhs) {
                    force!(rhs, Role::RightSide(op), [rhs.get_loc(), None], ExprStorage {
                        tok: ExprType::BinOp(op, lhs, rhs),
                        loc
                    });
                }
                Ok(Self::resolve_binop(&lhs, op, &rhs, &loc)?.loc(loc))
            }
            ExprStorage {
                tok: ExprType::Switch(ref_expr, cases, default_case),
                loc,
            } => {
                // Each case compares its value with the switched value,
                // through a comparison made below, which adds its side to
                // the trace
                for index in 0..cases.len() {
                    force!(
                        cases[index].0,
                        Role::SwitchCase,
                        [cases[index].0.get_loc(), loc.clone()],
                        ExprStorage {
                            tok: ExprType::Switch(ref_expr, cases, default_case),
                            loc
                        }
                    );
                    force!(
                        ref_expr,
                        Role::SwitchValue,
                        [ref_expr.get_loc(), loc.clone()],
                        ExprStorage {
                            tok: ExprType::Switch(ref_expr, cases, default_case),
                            loc
                        }
                    );
                }
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
                // The value, if the first pattern looks into it
                if cases
                    .first()
                    .is_some_and(|(pattern, _)| Self::pattern_resolves(pattern))
                {
                    force!(
                        ref_expr,
                        Role::MatchValue,
                        [loc.clone(), None],
                        ExprStorage {
                            tok: ExprType::Match(ref_expr, cases),
                            loc
                        }
                    );
                }
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
        };
        Ok(Step::Next(next?))
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
