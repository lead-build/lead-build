mod export;
pub mod matcher;

use super::error::{Error, ErrorType, Loc, Result};
pub use export::Exportable;
pub use matcher::Matcher;
use std::{
    cell::{Ref, RefCell},
    collections::{BTreeMap, HashSet},
    fmt::{Debug, Display},
    rc::Rc,
};
use strum::EnumTryAs;

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
    fn new_from_bool(&self, value: bool) -> Self;
    fn new_from_string(value: impl ToString) -> Self;
}

pub trait ExprBuiltin<T, F>: Debug
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    fn get_name(&self) -> String;
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
pub type ExprSet<T, F> = BTreeMap<String, Expr<T, F>>;

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
pub struct ExprBuiltinWrapper<T, F>(String, Rc<dyn ExprBuiltin<T, F>>)
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
#[derive(Debug, PartialEq, Clone, Default, EnumTryAs)]
pub enum ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F>,
    F: Clone,
{
    Object(ExprSet<T, F>),
    List(Vec<Expr<T, F>>),
    Tuple(Vec<Expr<T, F>>),
    Concat(Vec<Expr<T, F>>),
    AttrSel(Expr<T, F>, Expr<T, F>),
    Value(T),
    Var(String),
    UnOp(ExprUnOp, Expr<T, F>),
    BinOp(ExprBinOp, Expr<T, F>, Expr<T, F>),
    FuncDef(Matcher<T, F>, Expr<T, F>),
    FuncDefBuiltin(ExprBuiltinWrapper<T, F>),
    Let(Vec<(Matcher<T, F>, Expr<T, F>)>, Expr<T, F>),
    Fold(Expr<T, F>, Expr<T, F>, Expr<T, F>),
    Map(ExprMapType, Expr<T, F>, Expr<T, F>, Option<Expr<T, F>>),
    FuncCall(Expr<T, F>, Expr<T, F>),
    Bind(ExprSet<T, F>, Expr<T, F>),
    Switch(
        Expr<T, F>,
        Vec<(Expr<T, F>, Expr<T, F>)>,
        Option<Expr<T, F>>,
    ),
    #[default]
    Null,
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
    F: Clone + Debug,
{
    pub fn get_loc(&self) -> Option<Loc<F>> {
        self.inner_ref().loc.clone()
    }
}

impl<T, F> ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug,
{
    pub fn reref(self: ExprType<T, F>, loc: Option<Loc<F>>) -> Expr<T, F> {
        Expr(Rc::new(RefCell::new(ExprStorage { tok: self, loc })))
    }

    pub fn toexpr(self: ExprType<T, F>, left: usize, right: usize, f: &F) -> Expr<T, F> {
        self.reref(Some(Loc {
            file: f.clone(),
            left,
            right,
        }))
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
    F: Clone + Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.export(0, f)
    }
}

impl<T, F> Display for ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.export(0, f)
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
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug,
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
    F: Clone + Debug,
{
    pub fn bind(&self, varspace: &ExprSet<T, F>) -> Expr<T, F> {
        let referenced = self.referenced_vars();
        let filtered_varspace = varspace
            .iter()
            .filter(|(name, _)| referenced.contains(*name))
            .map(|(name, expr)| (name.clone(), expr.clone()))
            .collect();
        ExprType::Bind(filtered_varspace, self.clone()).reref(self.get_loc())
    }

    pub fn inner_ref(&self) -> Ref<'_, ExprStorage<T, F>> {
        self.0.as_ref().borrow()
    }

    pub fn referenced_vars(&self) -> HashSet<String> {
        match self.inner_ref().tok.clone() {
            ExprType::Object(fields) => fields
                .values()
                .flat_map(|field| field.referenced_vars().into_iter())
                .collect(),
            ExprType::List(items) | ExprType::Tuple(items) | ExprType::Concat(items) => items
                .iter()
                .flat_map(|item| item.referenced_vars().into_iter())
                .collect(),
            ExprType::AttrSel(value, attr) => {
                let mut vars = value.referenced_vars();
                vars.extend(attr.referenced_vars());
                vars
            }
            ExprType::Value(_) | ExprType::FuncDefBuiltin(_) | ExprType::Null => HashSet::new(),
            ExprType::Var(name) => HashSet::from([name]),
            ExprType::UnOp(_, inner) => inner.referenced_vars(),
            ExprType::BinOp(_, lhs, rhs) => {
                let mut vars = lhs.referenced_vars();
                vars.extend(rhs.referenced_vars());
                vars
            }
            ExprType::FuncDef(matcher, body) => {
                let mut vars = matcher.referenced_vars();
                vars.extend(body.referenced_vars());
                vars
            }
            ExprType::Let(bindings, target) => {
                let mut vars = HashSet::new();

                for (matcher, value_expr) in bindings.iter() {
                    vars.extend(value_expr.referenced_vars());
                    vars.extend(matcher.referenced_vars());
                }

                vars.extend(target.referenced_vars());
                vars
            }
            ExprType::Fold(func, init, input) => {
                let mut vars = func.referenced_vars();
                vars.extend(init.referenced_vars());
                vars.extend(input.referenced_vars());
                vars
            }
            ExprType::Map(_, func, input, filter) => {
                let mut vars = func.referenced_vars();
                vars.extend(input.referenced_vars());
                if let Some(filter_expr) = filter {
                    vars.extend(filter_expr.referenced_vars());
                }
                vars
            }
            ExprType::FuncCall(arg, func) => {
                let mut vars = arg.referenced_vars();
                vars.extend(func.referenced_vars());
                vars
            }
            ExprType::Bind(varspace, bound_expr) => {
                let mut vars: HashSet<String> = varspace
                    .values()
                    .flat_map(|value| value.referenced_vars().into_iter())
                    .collect();
                vars.extend(bound_expr.referenced_vars());
                vars
            }
            ExprType::Switch(ref_expr, cases, default_case) => {
                let mut vars = ref_expr.referenced_vars();
                for (matcher_expr, outcome_expr) in cases.iter() {
                    vars.extend(matcher_expr.referenced_vars());
                    vars.extend(outcome_expr.referenced_vars());
                }
                if let Some(default_expr) = default_case {
                    vars.extend(default_expr.referenced_vars());
                }
                vars
            }
        }
    }

    pub fn resolve(&self) -> Result<(), F> {
        let mut storref: ExprStorage<T, F> = self.inner_ref().clone();

        let todo = |loc, f, l, c| {
            Error::new(
                ErrorType::Type,
                format!("not yet implemented {}:{}:{}", f, l, c),
            )
            .reref(&loc)
        };

        while match &storref.tok {
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
            ExprType::Fold(..) => true,
            ExprType::Map(..) => true,
            ExprType::FuncCall(..) => true,
            ExprType::Bind(..) => true,
            ExprType::Switch(..) => true,
            ExprType::Null => false,
        } {
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
                            .map(|(k, val)| (k.clone(), val.bind(&varspace)))
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
                            for (name, value) in
                                field_matcher.run(field_expr.bind(&vars))?.into_iter()
                            {
                                vars.insert(name.clone(), value).map_or_else(
                                    || Ok(()),
                                    |_| Err(Error::new(ErrorType::DupKey, name.clone())),
                                )?;
                            }
                        }
                        Ok(target_expr.bind(&vars).inner_ref().clone())
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
                            func_expr.bind(&varspace),
                        )
                        .loc(loc))
                    }
                    ExprStorage {
                        tok: ExprType::FuncDefBuiltin(expr_builtin),
                        loc: biloc,
                    } => Ok(ExprType::FuncDefBuiltin(expr_builtin.clone()).loc(biloc.clone())),
                    ExprStorage {
                        tok: ExprType::Fold(func, init, input),
                        ..
                    } => Ok(ExprType::Fold(
                        func.bind(&varspace),
                        init.bind(&varspace),
                        input.bind(&varspace),
                    )
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
                            storref.loc = value.get_loc();
                            Ok(value
                                .res_type()
                                .map_err(|e| e.reref(&loc))?
                                .tok
                                .clone()
                                .loc(loc))
                        }
                        None => Err(Error::new(
                            ErrorType::Scope,
                            format!("Unknown variable {}", name),
                        )
                        .reref(vloc)),
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
                        tok: ExprType::FuncCall(fargs, fexpr),
                        ..
                    } => Ok(
                        ExprType::FuncCall(fargs.bind(&varspace), fexpr.bind(&varspace)).loc(loc),
                    ),
                    ExprStorage {
                        tok: ExprType::Value(value),
                        ..
                    } => Ok(ExprType::Value(value.clone()).loc(loc)),
                    ExprStorage {
                        tok: ExprType::Bind(inner_vars, inner_expr),
                        ..
                    } => Ok(inner_expr.bind(inner_vars).inner_ref().clone()),
                    ExprStorage {
                        tok: ExprType::Switch(inner_expr, inner_cases, inner_default),
                        ..
                    } => Ok(ExprType::Switch(
                        inner_expr.bind(&varspace),
                        inner_cases
                            .iter()
                            .map(|(m, e)| (m.bind(&varspace), e.bind(&varspace)))
                            .collect(),
                        inner_default.as_ref().map(|e| e.bind(&varspace)),
                    )
                    .loc(loc)),
                    ExprStorage {
                        tok: ExprType::Null,
                        ..
                    } => panic!("Found null in expr tree"),
                },
                ExprStorage {
                    tok: ExprType::Concat(parts),
                    loc,
                } => {
                    let part_values = parts
                        .iter()
                        .map(|part| part.value().map_err(|e| e.reref(&loc)))
                        .collect::<Result<Vec<T>, F>>()?;
                    Ok(ExprType::Value(T::op_string_concat(part_values)?).loc(loc))
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
                    let value = val.get_item(attr.value()?.as_string()?.as_str())?;
                    value.resolve()?;
                    storref.loc = value.inner_ref().loc.clone();
                    Ok(value.inner_ref().tok.clone().loc(loc))
                }
                ExprStorage {
                    tok: ExprType::FuncCall(fargs, fexpr),
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

                        Ok(fexpr.bind(&fbound).inner_ref().clone())
                    }
                    ExprStorage {
                        tok: ExprType::FuncDefBuiltin(ExprBuiltinWrapper(_, funcrc)),
                        ..
                    } => {
                        let empty_vars = ExprSet::new();
                        Ok(funcrc
                            .as_ref()
                            .call(fargs)
                            .map_err(|e| e.reref(&loc))?
                            .bind(&empty_vars)
                            .inner_ref()
                            .clone())
                    }
                    ExprStorage { tok: _, loc: floc } => Err(Error::new(
                        ErrorType::Scope,
                        format!("called func, but it's a {}", fexpr),
                    )
                    .reref(floc)),
                },
                ExprStorage {
                    tok: ExprType::Fold(func, init, input),
                    loc,
                } => {
                    let mut output = init;
                    match &*input.res_type()? {
                        ExprStorage {
                            tok: ExprType::List(input_items),
                            loc: input_loc,
                        } => {
                            for item in input_items.iter() {
                                output = ExprType::FuncCall(
                                    item.clone(),
                                    ExprType::FuncCall(output, func.clone())
                                        .reref(input_loc.clone()),
                                )
                                .reref(item.get_loc());
                            }
                            Ok(output.inner_ref().clone())
                        }
                        _ => Err(Error::new(
                            ErrorType::Eval,
                            format!("Fold over non-list: {}", input),
                        )
                        .reref(&loc)),
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
                                        ExprType::Value(T::new_from_string(k)).reref(v.get_loc()),
                                        v.clone(),
                                    ])
                                    .reref(v.get_loc())
                                })
                                .collect::<Vec<_>>();
                            Ok(args)
                        }
                        _ => Err(Error::new(
                            ErrorType::Eval,
                            format!("Foreach over non-iterable: {}", input),
                        )
                        .reref(&loc)),
                    }?;

                    let mut filtered_items: Vec<Expr<T, F>>;
                    if let Some(filter_expr) = filter {
                        filtered_items = Vec::new();
                        for item in input_items.iter() {
                            let filter_result =
                                ExprType::FuncCall(item.clone(), filter_expr.clone())
                                    .reref(item.get_loc())
                                    .value()?
                                    .as_bool()?;
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
                            ExprType::FuncCall(iel, func.clone()).reref(loc)
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
                                    } if els.len() == 2 => {
                                        Ok((els[0].value()?.as_string()?, els[1].clone()))
                                    }
                                    _ => Err(Error::new(
                                        ErrorType::Type,
                                        "Expecting tuple of 2 elements",
                                    )
                                    .reref(&el.get_loc())),
                                })
                                .collect::<Result<BTreeMap<String, Expr<T, F>>, F>>()?,
                        ),
                    };
                    Ok(output.loc(loc))
                }
                ExprStorage {
                    tok: ExprType::UnOp(op, expr),
                    loc,
                } => {
                    expr.resolve().map_err(|e| e.reref(&loc))?;
                    match op {
                        ExprUnOp::Neg => match &*expr.inner_ref() {
                            ExprStorage {
                                tok: ExprType::Value(value),
                                ..
                            } => Ok(ExprType::Value(value.op_neg()?).loc(loc)),
                            _ => Err(Error::new(
                                ErrorType::Eval,
                                format!("negating non-value: {}", expr),
                            )
                            .reref(&loc)),
                        },
                        ExprUnOp::Not => match &*expr.inner_ref() {
                            ExprStorage {
                                tok: ExprType::Value(value),
                                ..
                            } => Ok(ExprType::Value(value.op_not()?).loc(loc)),
                            _ => Err(Error::new(
                                ErrorType::Eval,
                                format!("negating non-value: {}", expr),
                            )
                            .reref(&loc)),
                        },
                    }
                }
                ExprStorage {
                    tok: ExprType::BinOp(op, lhs, rhs),
                    loc,
                } => match &*lhs.res_type().map_err(|e| e.reref(&loc))? {
                    ExprStorage {
                        tok: ExprType::Object(lhs_obj),
                        ..
                    } => match (op, &*rhs.res_type().map_err(|e| e.reref(&loc))?) {
                        (
                            ExprBinOp::Update,
                            ExprStorage {
                                tok: ExprType::Object(rhs_obj),
                                ..
                            },
                        ) => {
                            let mut res_obj = lhs_obj.clone();
                            res_obj.extend(
                                rhs_obj
                                    .iter()
                                    .map(|(key, value)| (key.clone(), value.clone())),
                            );
                            Ok(ExprType::Object(res_obj).loc(loc))
                        }
                        (ExprBinOp::Update, ExprStorage { tok: rhs_tok, .. }) => Err(Error::new(
                            ErrorType::Type,
                            format!("Object cannot be updated with {}", rhs_tok),
                        )
                        .reref(&loc)),
                        _ => Err(todo(loc, file!(), line!(), column!())),
                    },
                    ExprStorage {
                        tok: ExprType::List(lhs_list),
                        ..
                    } => match (op, &*rhs.res_type().map_err(|e| e.reref(&loc))?) {
                        (
                            ExprBinOp::ListConcat,
                            ExprStorage {
                                tok: ExprType::List(rhs_list),
                                ..
                            },
                        ) => {
                            let mut res = lhs_list.clone();
                            res.extend(rhs_list.iter().cloned());
                            Ok(ExprType::List(res).loc(loc))
                        }
                        _ => Err(todo(loc, file!(), line!(), column!())),
                    },
                    ExprStorage {
                        tok: ExprType::Value(lhs_val),
                        loc: lhs_loc,
                    } => match op {
                        ExprBinOp::LogAnd => match lhs_val.as_bool()? {
                            true => Ok(rhs
                                .res_type()
                                .map_err(|e| e.reref(lhs_loc))?
                                .tok
                                .clone()
                                .loc(loc)),
                            false => Ok(ExprType::Value(lhs_val.new_from_bool(false)).loc(loc)),
                        },
                        ExprBinOp::LogOr => match lhs_val.as_bool()? {
                            true => Ok(ExprType::Value(lhs_val.new_from_bool(true)).loc(loc)),
                            false => Ok(rhs
                                .res_type()
                                .map_err(|e| e.reref(lhs_loc))?
                                .tok
                                .clone()
                                .loc(loc)),
                        },
                        ExprBinOp::LogImpl => match lhs_val.as_bool()? {
                            false => Ok(ExprType::Value(lhs_val.new_from_bool(true)).loc(loc)),
                            true => Ok(rhs
                                .res_type()
                                .map_err(|e| e.reref(lhs_loc))?
                                .tok
                                .clone()
                                .loc(loc)),
                        },
                        _ => match &*rhs.res_type().map_err(|e| e.reref(lhs_loc))? {
                            ExprStorage {
                                tok: ExprType::Object(_rhs_obj),
                                loc: rhs_loc,
                            } => Err(todo(rhs_loc.clone(), file!(), line!(), column!())),
                            ExprStorage {
                                tok: ExprType::Value(rhs_val),
                                loc: _rhs_loc,
                            } => match op {
                                ExprBinOp::HasAttr => {
                                    Err(todo(loc.clone(), file!(), line!(), column!()))
                                }
                                ExprBinOp::ListConcat => {
                                    Err(todo(loc.clone(), file!(), line!(), column!()))
                                }
                                ExprBinOp::Mult => {
                                    Ok(ExprType::Value(T::op_mult(lhs_val, rhs_val)?).loc(loc))
                                }
                                ExprBinOp::Div => {
                                    Ok(ExprType::Value(T::op_div(lhs_val, rhs_val)?).loc(loc))
                                }
                                ExprBinOp::Sub => {
                                    Ok(ExprType::Value(T::op_sub(lhs_val, rhs_val)?).loc(loc))
                                }
                                ExprBinOp::Add => {
                                    Ok(ExprType::Value(T::op_add(lhs_val, rhs_val)?).loc(loc))
                                }
                                ExprBinOp::Update => {
                                    Err(todo(loc.clone(), file!(), line!(), column!()))
                                }
                                ExprBinOp::Lt => {
                                    Ok(ExprType::Value(T::op_lt(lhs_val, rhs_val)?).loc(loc))
                                }
                                ExprBinOp::Le => {
                                    Ok(ExprType::Value(T::op_le(lhs_val, rhs_val)?).loc(loc))
                                }
                                ExprBinOp::Gt => {
                                    Ok(ExprType::Value(T::op_gt(lhs_val, rhs_val)?).loc(loc))
                                }
                                ExprBinOp::Ge => {
                                    Ok(ExprType::Value(T::op_ge(lhs_val, rhs_val)?).loc(loc))
                                }
                                ExprBinOp::Eq => {
                                    Ok(ExprType::Value(T::op_eq(lhs_val, rhs_val)?).loc(loc))
                                }
                                ExprBinOp::Neq => {
                                    Ok(ExprType::Value(T::op_neq(lhs_val, rhs_val)?).loc(loc))
                                }
                                _ => unreachable!(),
                            },
                            ExprStorage { tok, loc: _ } => Err(Error::new(
                                ErrorType::Eval,
                                format!("Resolving unresolvable type {}", tok),
                            )
                            .reref(&loc)),
                        },
                    },
                    ExprStorage { tok, .. } => Err(Error::new(
                        ErrorType::Eval,
                        format!("Resolving unresolvable type {}", tok),
                    )
                    .reref(&loc)),
                },
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
                                format!("No matching case for {}", ref_expr),
                            )
                            .reref(&loc))
                        }
                    }
                }
                ExprStorage {
                    tok: ExprType::Var(name),
                    loc,
                } => Err(
                    Error::new(ErrorType::Scope, format!("Unknown variable {}", name)).reref(&loc),
                ),
                ExprStorage {
                    tok: ExprType::Null,
                    loc: _loc,
                } => panic!("Found null in expr tree"),
                ExprStorage { tok, loc: _ } => unreachable!("Resolving {}", tok),
            }?;
        }

        self.0.as_ref().replace(storref);
        Ok(())
    }

    fn res_type(&self) -> Result<Ref<'_, ExprStorage<T, F>>, F> {
        self.resolve()?;
        Ok(self.inner_ref())
    }

    pub fn eval(&self) -> Result<(), F> {
        let mut first_err: Option<Error<F>> = None;

        if let Err(err) = self.resolve() {
            first_err = Some(err);
        }

        let fields: Vec<Expr<T, F>> = match &self.inner_ref().tok {
            ExprType::Object(fields) => fields.values().cloned().collect(),
            ExprType::List(fields) | ExprType::Tuple(fields) | ExprType::Concat(fields) => {
                fields.to_vec()
            }
            ExprType::Bind(varspace, bound_expr) => {
                let mut parts: Vec<Expr<T, F>> = varspace.values().cloned().collect();
                parts.push(bound_expr.clone());
                parts
            }
            _ => vec![],
        };

        for ex in fields.into_iter() {
            if let Err(err) = ex.eval()
                && first_err.is_none()
            {
                first_err = Some(err);
            }
        }

        if let Some(err) = first_err {
            Err(err)
        } else {
            Ok(())
        }
    }

    pub fn value(&self) -> Result<T, F> {
        // Since we expect a string, we only need to resolve one level.
        self.resolve()?;
        match &self.inner_ref().tok {
            ExprType::Value(val) => Ok(val.clone()),
            _ => Err(Error::new(
                ErrorType::NoValue,
                format!("Not a value: {}", self),
            )),
        }
    }

    pub fn eval_string(&self) -> Result<String, F> {
        // Since we expect a string, we only need to resolve one level.
        self.resolve()?;
        match &self.inner_ref().tok {
            ExprType::Value(val) => Ok(val.as_string()?),
            _ => Err(Error::new(
                ErrorType::NoValue,
                format!("Not a string: {}", self),
            )),
        }
    }

    pub fn get_item(&self, name: &str) -> Result<Expr<T, F>, F> {
        self.resolve()?;
        let node = self.inner_ref();
        match &node.tok {
            ExprType::Object(vars) => Ok(vars
                .get(name)
                .ok_or_else(|| Error::new(ErrorType::NoValue, format!("Invalid field '{}'", name)))?
                .clone()),
            _ => Err(Error::new(
                ErrorType::NoValue,
                format!("Missing item '{}'", name),
            )),
        }
    }

    pub fn new_builtin(func: Rc<dyn ExprBuiltin<T, F>>) -> Expr<T, F> {
        ExprType::FuncDefBuiltin(ExprBuiltinWrapper(func.as_ref().get_name(), func)).builtin()
    }

    pub fn from_builtins(value: Vec<Rc<dyn ExprBuiltin<T, F>>>) -> Expr<T, F> {
        let mut exprset = ExprSet::new();

        for bi in value.into_iter() {
            let name = bi.get_name();
            exprset
                .insert(
                    name.clone(),
                    ExprType::FuncDefBuiltin(ExprBuiltinWrapper(name, bi)).builtin(),
                )
                .unwrap();
        }

        ExprType::Object(exprset).builtin()
    }
}
