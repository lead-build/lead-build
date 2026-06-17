use std::{
    cell::{Ref, RefCell},
    fmt::{Debug, Display},
    rc::Rc,
};

use super::immap::ImMap;

pub mod ops {
    use super::{Debug, Display, Expr};

    pub trait ExprOps: Sized {
        fn op_add(lhs: &Self, rhs: &Self) -> Result<Self>;
        fn op_sub(lhs: &Self, rhs: &Self) -> Result<Self>;
        fn op_mult(lhs: &Self, rhs: &Self) -> Result<Self>;
        fn op_div(lhs: &Self, rhs: &Self) -> Result<Self>;
        fn op_lt(lhs: &Self, rhs: &Self) -> Result<Self>;
        fn op_le(lhs: &Self, rhs: &Self) -> Result<Self>;
        fn op_gt(lhs: &Self, rhs: &Self) -> Result<Self>;
        fn op_ge(lhs: &Self, rhs: &Self) -> Result<Self>;
        fn op_eq(lhs: &Self, rhs: &Self) -> Result<Self>;
        fn op_neq(lhs: &Self, rhs: &Self) -> Result<Self>;
        fn op_neg(&self) -> Result<Self>;
        fn op_not(&self) -> Result<Self>;
        fn as_bool(&self) -> Result<bool>;
        fn as_string(&self) -> Result<String>;
        fn new_from_bool(&self, value: bool) -> Self;
    }

    pub trait ExprBuiltin<T>: Debug
    where
        T: Clone + PartialEq + Display + ExprOps,
    {
        fn get_name(&self) -> String;
        fn call(&self, arg: Expr<T>) -> Result<Expr<T>>;
    }

    pub enum Error {
        Type(String),
        ExprError(super::Error),
    }

    impl From<super::Error> for Error {
        fn from(value: super::Error) -> Self {
            Error::ExprError(value)
        }
    }

    impl From<super::super::immap::Error> for Error {
        fn from(value: super::super::immap::Error) -> Self {
            Error::ExprError(value.into())
        }
    }

    impl From<super::super::error::Error> for Error {
        fn from(value: super::super::error::Error) -> Self {
            Error::ExprError(value.into())
        }
    }

    pub type Result<T> = std::result::Result<T, Error>;
}

use ops::{ExprBuiltin, ExprOps};
use strum::EnumTryAs;

/*
 * Error
 */

#[derive(Debug)]
pub enum Error {
    Scope(String),
    Eval(String),
    Type(String),
    DupKey(String),
    NoValue(String),
    Lang(Rc<super::error::Error>),
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Scope(msg) => write!(f, "ScopeError: {}", msg),
            Error::Eval(msg) => write!(f, "EvalError: {}", msg),
            Error::Type(msg) => write!(f, "TypeError: {}", msg),
            Error::DupKey(msg) => write!(f, "DupKey: {}", msg),
            Error::NoValue(msg) => write!(f, "No value: {}", msg),
            Error::Lang(dnj_error) => Display::fmt(&dnj_error, f),
        }
    }
}

impl From<super::immap::Error> for Error {
    fn from(value: super::immap::Error) -> Self {
        match value {
            super::immap::Error::DupKey(key) => Error::DupKey(key),
        }
    }
}

impl From<ops::Error> for Error {
    fn from(value: ops::Error) -> Self {
        match value {
            ops::Error::Type(msg) => Error::Type(msg),
            ops::Error::ExprError(err) => err,
        }
    }
}

impl From<super::error::Error> for Error {
    fn from(value: super::error::Error) -> Self {
        Error::Lang(value.into())
    }
}

type Result<RT> = std::result::Result<RT, Error>;

/* *****************************************************************************
 * Types
 */

#[derive(Debug, PartialEq, Clone)]
pub struct Expr<T>(Rc<RefCell<ExprType<T>>>)
where
    T: Clone + PartialEq + Display + ExprOps;

// TODO: Better implementation of ExprSet... This probably takes time to clone.
pub type ExprSet<T> = ImMap<Expr<T>>;

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

#[derive(Clone)]
pub struct ExprBuiltinWrapper<T>(String, Rc<dyn ExprBuiltin<T>>)
where
    T: Clone + PartialEq + Display + ExprOps;

// Clone is needed since ExprType::Var is implemented via cloning of ExprType
#[derive(Debug, PartialEq, Clone, Default, EnumTryAs)]
pub enum ExprType<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    Object(ExprSet<T>),
    List(Vec<Expr<T>>),
    AttrSel(Expr<T>, String),
    Value(T),
    Var(String),
    UnOp(ExprUnOp, Expr<T>),
    BinOp(ExprBinOp, Expr<T>, Expr<T>),
    FuncDefIdent(String, Expr<T>),
    FuncDefPattern(Vec<String>, Expr<T>),
    FuncDefBuiltin(ExprBuiltinWrapper<T>),
    Let(Vec<(String, Expr<T>)>, Expr<T>),
    MapList(Expr<T>, Expr<T>),
    FuncCall(Expr<T>, Expr<T>),
    BoundExpr(ExprSet<T>, Expr<T>),
    #[default]
    Null,
}

/* *****************************************************************************
 * Display
 */

impl<T> Debug for ExprBuiltinWrapper<T>
where
    T: Clone + PartialEq + Display + ExprOps,
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

impl<T> Display for Expr<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.as_ref().borrow().fmt(f)
    }
}

impl<T> Display for ExprType<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExprType::Object(varscope) => varscope.fmt(f),
            ExprType::List(items) => {
                write!(f, "[")?;
                for item in items.iter() {
                    write!(f, " {}", item)?;
                }
                write!(f, " ]")?;
                Ok(())
            }
            ExprType::AttrSel(val, attr) => write!(f, "{}.{}", val, attr),
            ExprType::Value(val) => val.fmt(f),
            ExprType::Var(val) => Display::fmt(&val, f),
            ExprType::UnOp(op, expr) => {
                write!(f, "{}({})", op, expr)
            }
            ExprType::BinOp(op, lhs, rhs) => {
                write!(f, "({}){}({})", lhs, op, rhs)
            }
            ExprType::FuncDefIdent(name, expr) => write!(f, "{}: {}", name, expr),
            ExprType::FuncDefPattern(items, expr) => {
                f.write_str("{")?;
                for item in items {
                    Display::fmt(&item, f)?;
                    f.write_str(", ")?;
                }
                f.write_str("...}: ")?;
                expr.fmt(f)?;
                Ok(())
            }
            ExprType::Let(items, expr) => {
                f.write_str("let ")?;
                for (var_name, var_expr) in items {
                    std::fmt::Display::fmt(&var_name, f)?;
                    f.write_str("=")?;
                    std::fmt::Display::fmt(&var_expr, f)?;
                    f.write_str("; ")?;
                }
                f.write_str("in ")?;
                expr.fmt(f)?;
                Ok(())
            }
            ExprType::MapList(func, input) => {
                write!(f, "[ {} <- {} ]", func, input)
            }
            ExprType::FuncCall(fexpr, farg) => write!(f, "{} {}", fexpr, farg),
            ExprType::BoundExpr(scope, expr) => write!(f, "[ {} @ {} ]", scope, expr),
            ExprType::FuncDefBuiltin(ExprBuiltinWrapper(name, _)) => {
                write!(f, "<builtin {}>", name)
            }
            ExprType::Null => write!(f, "null"),
        }
    }
}

/* *****************************************************************************
 * Transform / From
 */

impl<T> From<T> for ExprType<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn from(value: T) -> Self {
        ExprType::Value(value)
    }
}

impl<T> From<ExprType<T>> for Expr<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn from(value: ExprType<T>) -> Self {
        Expr(Rc::new(RefCell::new(value)))
    }
}

impl<T> From<T> for Expr<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn from(value: T) -> Self {
        Expr::from(ExprType::Value(value))
    }
}

impl<T> From<ExprSet<T>> for Expr<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn from(value: ExprSet<T>) -> Self {
        Expr::from(ExprType::Object(value))
    }
}

/* *****************************************************************************
 * Implementations
 */

impl<T> PartialEq for ExprBuiltinWrapper<T>
where
    T: Clone + PartialEq + Display + ExprOps,
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

impl<T> Expr<T>
where
    T: Clone + PartialEq + Display + ExprOps + Debug,
{
    pub fn as_ref(&self) -> Ref<'_, ExprType<T>> {
        self.0.as_ref().borrow()
    }

    pub fn resolve(&self) -> Result<()> {
        let mut expr = self.0.as_ref().take();

        while match &expr {
            ExprType::Object(..) => false,
            ExprType::List(..) => false,
            ExprType::AttrSel(..) => true,
            ExprType::Value(..) => false,
            ExprType::Var(..) => true,
            ExprType::UnOp(..) => true,
            ExprType::BinOp(..) => true,
            ExprType::FuncDefIdent(..) => false,
            ExprType::FuncDefPattern(..) => false,
            ExprType::FuncDefBuiltin(..) => false,
            ExprType::Let(..) => true,
            ExprType::MapList(..) => true,
            ExprType::FuncCall(..) => true,
            ExprType::BoundExpr(..) => true,
            ExprType::Null => false,
        } {
            expr = match expr {
                ExprType::BoundExpr(varspace, bound_expr) => match &*bound_expr.as_ref() {
                    ExprType::Object(fields) => {
                        Ok(ExprType::Object(fields.map(|val| {
                            ExprType::BoundExpr(varspace.clone(), val.clone()).into()
                        })))
                    }
                    ExprType::List(items) => Ok(ExprType::List(
                        items
                            .iter()
                            .map(|item| ExprType::BoundExpr(varspace.clone(), item.clone()).into())
                            .collect(),
                    )),
                    ExprType::AttrSel(val, attr) => Ok(ExprType::AttrSel(
                        ExprType::BoundExpr(varspace, val.clone()).into(),
                        attr.clone(),
                    )),
                    ExprType::Let(fields, target_expr) => {
                        let mut vars: ExprSet<T> = varspace;
                        for (field_name, field_expr) in fields {
                            let field_vars = vars.clone();
                            vars = vars.set(
                                field_name,
                                ExprType::BoundExpr(field_vars, field_expr.clone()).into(),
                            )?;
                        }
                        Ok(ExprType::BoundExpr(vars, target_expr.clone()))
                    }
                    ExprType::FuncDefIdent(arg_name, func_expr) => {
                        let new_scope = varspace.unset(arg_name.as_str());
                        Ok(ExprType::FuncDefIdent(
                            arg_name.clone(),
                            ExprType::BoundExpr(new_scope, func_expr.clone()).into(),
                        ))
                    }
                    ExprType::FuncDefPattern(items, expr) => {
                        let mut new_scope = varspace;
                        for item in items.iter() {
                            new_scope = new_scope.unset(item);
                        }
                        Ok(ExprType::FuncDefPattern(
                            items.clone(),
                            ExprType::BoundExpr(new_scope, expr.clone()).into(),
                        ))
                    }
                    ExprType::FuncDefBuiltin(_expr_builtin) => todo!(),
                    ExprType::MapList(func, input) => Ok(ExprType::MapList(
                        ExprType::BoundExpr(varspace.clone(), func.clone()).into(),
                        ExprType::BoundExpr(varspace.clone(), input.clone()).into(),
                    )),
                    ExprType::Var(name) => match &varspace.get(name) {
                        Some(value) => Ok(value.res_type()?.clone()),
                        None => Err(Error::Scope(format!(
                            "Unknown variable {} in {}",
                            name, varspace
                        ))),
                    },
                    ExprType::UnOp(op, expr) => Ok(ExprType::UnOp(
                        *op,
                        ExprType::BoundExpr(varspace, expr.clone()).into(),
                    )),
                    ExprType::BinOp(op, lhs, rhs) => Ok(ExprType::BinOp(
                        *op,
                        ExprType::BoundExpr(varspace.clone(), lhs.clone()).into(),
                        ExprType::BoundExpr(varspace, rhs.clone()).into(),
                    )),
                    ExprType::FuncCall(fexpr, fargs) => Ok(ExprType::FuncCall(
                        ExprType::BoundExpr(varspace.clone(), fexpr.clone()).into(),
                        ExprType::BoundExpr(varspace, fargs.clone()).into(),
                    )),
                    ExprType::Value(value) => Ok(ExprType::Value(value.clone())),
                    ExprType::BoundExpr(inner_vars, inner_expr) => {
                        Ok(ExprType::BoundExpr(inner_vars.clone(), inner_expr.clone()))
                    }
                    ExprType::Null => panic!("Found null in expr tree"),
                },
                ExprType::AttrSel(val, attr) => {
                    let attr_expr = val.get_item(attr.as_str())?;
                    Ok(attr_expr.as_ref().clone())
                }
                ExprType::FuncCall(fexpr, fargs) => {
                    let (args, func_expr): (ExprSet<T>, Expr<T>) = match &*fexpr.res_type()? {
                        ExprType::FuncDefIdent(arg_name, fimpl) => {
                            Ok((ExprSet::single(arg_name, fargs), fimpl.clone()))
                        }
                        ExprType::FuncDefPattern(arg_names, fimpl) => {
                            fargs.resolve()?;
                            let mut new_vars = ExprSet::new();
                            for arg_name in arg_names {
                                let arg_value = fargs.get_item(arg_name)?;
                                new_vars = new_vars.set(arg_name, arg_value)?;
                            }
                            Ok((new_vars, fimpl.clone()))
                        }
                        ExprType::FuncDefBuiltin(ExprBuiltinWrapper(_, funcrc)) => {
                            let res = funcrc.as_ref().call(fargs)?;
                            Ok((ExprSet::new(), res))
                        }
                        _ => Err(Error::Scope(format!("called func, but it's a {}", fexpr))),
                    }?;

                    // If function contains a bound scope, it should still apply,
                    // and not overwrite input arguments.
                    match &*func_expr.as_ref() {
                        ExprType::BoundExpr(varspace, inner_expr) => Ok(ExprType::BoundExpr(
                            varspace.clone().merge(&args),
                            inner_expr.clone(),
                        )),
                        _ => Ok(ExprType::BoundExpr(args, func_expr.clone())),
                    }
                }
                ExprType::MapList(func, input) => {
                    input.resolve()?;
                    match &*input.as_ref() {
                        ExprType::List(input_vec) => Ok(ExprType::List(
                            input_vec
                                .iter()
                                .map(|iel| ExprType::FuncCall(func.clone(), iel.clone()).into())
                                .collect::<Vec<_>>(),
                        )),
                        _ => Err(Error::Eval(format!("Foreach over non-list: {}", input))),
                    }
                }
                ExprType::UnOp(op, expr) => {
                    expr.resolve()?;
                    match op {
                        ExprUnOp::Neg => match &*expr.as_ref() {
                            ExprType::Value(value) => Ok(ExprType::Value(value.op_neg()?)),
                            _ => Err(Error::Eval(format!("negating non-value: {}", expr))),
                        },
                        ExprUnOp::Not => match &*expr.as_ref() {
                            ExprType::Value(value) => Ok(ExprType::Value(value.op_not()?)),
                            _ => Err(Error::Eval(format!("negating non-value: {}", expr))),
                        },
                    }
                }
                ExprType::BinOp(op, lhs, rhs) => match &*lhs.res_type()? {
                    ExprType::Object(lhs_obj) => todo!("Binop on {}", lhs_obj),
                    ExprType::List(lhs_list) => match (op, &*rhs.res_type()?) {
                        (ExprBinOp::Add, ExprType::List(rhs_list)) => {
                            let mut res = lhs_list.clone();
                            res.extend(rhs_list.iter().cloned());
                            Ok(ExprType::List(res))
                        }
                        _ => todo!("error message"),
                    },
                    ExprType::Value(lhs_val) => match op {
                        ExprBinOp::LogAnd => match lhs_val.as_bool()? {
                            true => Ok(rhs.res_type()?.clone()),
                            false => Ok(ExprType::Value(lhs_val.new_from_bool(false))),
                        },
                        ExprBinOp::LogOr => match lhs_val.as_bool()? {
                            true => Ok(ExprType::Value(lhs_val.new_from_bool(true))),
                            false => Ok(rhs.res_type()?.clone()),
                        },
                        ExprBinOp::LogImpl => match lhs_val.as_bool()? {
                            false => Ok(ExprType::Value(lhs_val.new_from_bool(true))),
                            true => Ok(rhs.res_type()?.clone()),
                        },
                        _ => match &*rhs.res_type()? {
                            ExprType::Object(_rhs_obj) => todo!(),
                            ExprType::Value(rhs_val) => match op {
                                ExprBinOp::HasAttr => todo!(),
                                ExprBinOp::ListConcat => todo!(),
                                ExprBinOp::Mult => {
                                    Ok(ExprType::Value(T::op_mult(lhs_val, rhs_val)?))
                                }
                                ExprBinOp::Div => Ok(ExprType::Value(T::op_div(lhs_val, rhs_val)?)),
                                ExprBinOp::Sub => Ok(ExprType::Value(T::op_sub(lhs_val, rhs_val)?)),
                                ExprBinOp::Add => Ok(ExprType::Value(T::op_add(lhs_val, rhs_val)?)),
                                ExprBinOp::Update => todo!(),
                                ExprBinOp::Lt => Ok(ExprType::Value(T::op_lt(lhs_val, rhs_val)?)),
                                ExprBinOp::Le => Ok(ExprType::Value(T::op_le(lhs_val, rhs_val)?)),
                                ExprBinOp::Gt => Ok(ExprType::Value(T::op_gt(lhs_val, rhs_val)?)),
                                ExprBinOp::Ge => Ok(ExprType::Value(T::op_ge(lhs_val, rhs_val)?)),
                                ExprBinOp::Eq => Ok(ExprType::Value(T::op_eq(lhs_val, rhs_val)?)),
                                ExprBinOp::Neq => Ok(ExprType::Value(T::op_neq(lhs_val, rhs_val)?)),
                                _ => unreachable!(),
                            },
                            typ => Err(Error::Eval(format!("Resolving unresolvable type {}", typ))),
                        },
                    },
                    typ => Err(Error::Eval(format!("Resolving unresolvable type {}", typ))),
                },
                ExprType::Null => panic!("Found null in expr tree"),
                typ => unreachable!("Resolving {}", typ),
            }?;
        }

        self.0.as_ref().replace(expr);
        Ok(())
    }

    fn res_type(&self) -> Result<Ref<'_, ExprType<T>>> {
        self.resolve()?;
        Ok(self.as_ref())
    }

    pub fn eval(&self) -> Result<()> {
        self.resolve()?;
        match &*self.as_ref() {
            ExprType::Object(fields) => {
                fields.foreach(|_name, ex| ex.eval())?;
            }
            ExprType::List(fields) => {
                for ex in fields.iter() {
                    ex.eval()?
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn value(&self) -> Result<T> {
        // Since we expect a string, we only need to resolve one level.
        self.resolve()?;
        match &*self.as_ref() {
            ExprType::Value(val) => Ok(val.clone()),
            _ => Err(Error::NoValue(format!("Not a value: {}", self))),
        }
    }

    pub fn eval_string(&self) -> Result<String> {
        // Since we expect a string, we only need to resolve one level.
        self.resolve()?;
        match &*self.as_ref() {
            ExprType::Value(val) => Ok(val.as_string()?),
            _ => Err(Error::NoValue(format!("Not a string: {}", self))),
        }
    }

    pub fn get_item(&self, name: &str) -> Result<Expr<T>> {
        self.resolve()?;
        let node = self.as_ref();
        match &*node {
            ExprType::Object(vars) => Ok(vars
                .get(name)
                .ok_or(Error::NoValue(format!("Invalid field '{}'", name)))?),
            _ => Err(Error::NoValue(format!("Invalid item '{}'", name))),
        }
    }

    pub fn new_builtin(func: Rc<dyn ExprBuiltin<T>>) -> Expr<T> {
        ExprType::FuncDefBuiltin(ExprBuiltinWrapper(func.as_ref().get_name(), func)).into()
    }

    pub fn from_builtins(value: Vec<Rc<dyn ExprBuiltin<T>>>) -> Expr<T> {
        let mut exprset = ExprSet::new();

        for bi in value.into_iter() {
            let name = bi.get_name();
            exprset = exprset
                .set(
                    name.clone(),
                    ExprType::FuncDefBuiltin(ExprBuiltinWrapper(name, bi)).into(),
                )
                .unwrap();
        }

        exprset.into()
    }
}

impl<T> ExprType<T> where T: Clone + PartialEq + Display + ExprOps + Debug {}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{super::parser::parse_str, super::testvalue::TestValue, ExprType::BoundExpr};

    fn eval(code: &str) -> Expr<TestValue> {
        let expr: Expr<TestValue> =
            ExprType::BoundExpr(ExprSet::new(), parse_str(code).unwrap()).into();
        expr.eval().unwrap();
        expr
    }

    #[test]
    fn test_resolve() -> Result<()> {
        let expr = parse_str(
            r#"
                {
                    stuff = "hello";
                    something = "hej";
                }
            "#,
        )
        .unwrap();
        let value = expr.get_item("stuff")?;
        assert_eq!(value, Expr::from(TestValue::String("hello".into())));
        Ok(())
    }

    #[test]
    fn test_eval_obj() {
        assert_eq!(eval("{ a = (let x = 3; in x); }"), eval("{ a = 3; }"));
    }

    #[test]
    fn test_eval_list() {
        assert_eq!(eval("[ (let x = 3; in x) ]"), eval("[ 3 ]"));
    }

    #[test]
    fn test_func_in_let_res() {
        assert_eq!(eval("let a = x: (x+1); in (a 12)"), eval("13"));
    }

    #[test]
    fn test_resolve_deep() -> Result<()> {
        // This also tests "inner" as prefixed for reserved keyword "in" is ok
        let expr = parse_str(
            r#"
                {
                    stuff = "hello";
                    something = {
                        inner = 55;
                    };
                }
            "#,
        )
        .unwrap();
        let value = expr.get_item("something")?.get_item("inner")?;
        assert_eq!(value, Expr::from(TestValue::Int(55)));
        Ok(())
    }

    #[test]
    fn test_let() {
        let value = eval(
            r#"
                let
                    a = 12;
                    b = 75;
                in
                b
            "#,
        );
        assert_eq!(value, Expr::from(TestValue::Int(75)));
    }

    #[test]
    fn test_invalid_var() -> Result<()> {
        let expr: Expr<TestValue> =
            ExprType::BoundExpr(ExprSet::new(), parse_str("invalid_var").unwrap()).into();
        if let Err(Error::Scope(message)) = expr.resolve() {
            assert_eq!(message.as_str(), "Unknown variable invalid_var in { }");
        } else {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn test_let_set_var() {
        assert_eq! {
            eval(r#"
                let
                    a = 12;
                in
                {
                    stuff = a;
                }
            "#),
            eval("{ stuff = 12; }"),
        }
    }

    #[test]
    fn test_let_set_var_seq() {
        assert_eq! {
            eval(r#"
                let
                    a = 12;
                    b = a;
                in
                {
                    stuff = b;
                }
            "#),
            eval("{ stuff = 12; }"),
        }
    }

    #[test]
    fn test_func_call() {
        let func_a = parse_str("var: 13").unwrap();
        let func_b = parse_str("var: 42").unwrap();
        let call = parse_str("func_b 32").unwrap();
        let varscope =
            ExprSet::from(vec![("func_a", func_a.into()), ("func_b", func_b.into())]).unwrap();
        let value: Expr<TestValue> = ExprType::BoundExpr(varscope, call).into();
        value.resolve().unwrap();
        assert_eq!(value, Expr::from(TestValue::Int(42)));
    }

    #[test]
    fn test_func_call_var_arg() {
        let func_var = parse_str("var: var").unwrap();
        let arg_var = parse_str("32").unwrap();
        let call = parse_str("func arg").unwrap();
        let varscope =
            ExprSet::from(vec![("func", func_var.into()), ("arg", arg_var.into())]).unwrap();
        let value: Expr<TestValue> = ExprType::BoundExpr(varscope, call).into();
        value.resolve().unwrap();
        assert_eq!(value, Expr::from(TestValue::Int(32)));
    }

    #[test]
    fn test_func_call_order() {
        assert_eq! {
            eval("let f = a: b: (a+b); in (f 3 4)"),
            eval("7"),
        }
    }

    #[test]
    fn test_func_call_resolved() {
        assert_eq! {
            eval(r#"
                let
                    a = 12;
                    func = test: {
                        var = test;
                    };
                in
                {
                    stuff = func a;
                }
            "#),
            eval("{ stuff = { var = 12; }; }"),
        }
    }

    #[test]
    fn test_func_call_bound() {
        assert_eq! {
            eval(r#"
                let
                    a = 12;
                    func = test: {
                        var = a;
                    };
                in
                {
                    stuff = func 77;
                }
            "#),
            eval("{ stuff = { var = 12; }; }"),
        }
    }

    #[test]
    fn test_func_call_resolved_stacked_let() {
        assert_eq! {
            eval(r#"
                let
                    a = 12;
                in
                let
                    func = test: {
                        var = test;
                    };
                in
                {
                    stuff = func a;
                }
            "#),
            eval("{ stuff = { var = 12; }; }"),
        }
    }

    #[test]
    fn test_func_call_pattern() {
        assert_eq! {
            eval(r#"
                let
                    a = 12;
                    b = 13;
                    func = { a, b, ... }: {
                        var = b;
                    };
                in
                {
                    stuff = func {
                        a = 15;
                        b = 74;
                    };
                }
            "#),
            eval("{ stuff = { var = 74; }; }"),
        }
    }

    #[test]
    fn test_var_through_func_call() {
        // TODO: "let var = 3 in func var" should work, or give an error...
        assert_eq! {
            eval(r#"
                let
                    func = (a: a+2);
                in
                let
                    myvar = 13;
                in
                (func myvar)
            "#),
            eval("15"),
        }
    }

    #[test]
    fn test_eval() {
        assert_eq! {
            eval(r#"
                let
                    a = 12;
                    b = { inner = 43; };
                    myfunc = {target, ...}: { var = b; };
                in
                {
                    app = myfunc {
                        target = "app.elf";
                    };
                }
            "#),
            eval("{ app = { var = { inner = 43; }; }; }"),
        }
    }

    #[test]
    fn test_arith() {
        assert_eq! {
            eval("2 * 3 + 4 * 5"),
            eval("6 + 20"),
        }
        assert_eq! {
            eval("6 + 20"),
            eval("26"),
        }
    }

    #[test]
    fn test_bool_op() {
        assert_eq!(eval("false || 12"), eval("12"));
        assert_eq!(eval("true || 12"), eval("true"));
        assert_eq!(eval("false && 12"), eval("false"));
        assert_eq!(eval("true && 12"), eval("12"));
    }

    #[test]
    fn test_bool_laziness() {
        assert_eq!(eval("true || invalid_var"), eval("true"));
        assert_eq!(eval("false && invalid_var"), eval("false"));
        assert_eq!(eval("false -> invalid_var"), eval("true"));
    }

    #[test]
    fn test_bool_implication() {
        assert_eq!(eval("false -> false"), eval("true"));
        assert_eq!(eval("false -> true"), eval("true"));
        assert_eq!(eval("true -> false"), eval("false"));
        assert_eq!(eval("true -> true"), eval("true"));
        assert_eq!(eval("false -> 12"), eval("true"));
        assert_eq!(eval("true -> 12"), eval("12"));
    }

    #[test]
    fn test_bool_not() {
        assert_eq!(eval("!true"), eval("false"));
        assert_eq!(eval("!false"), eval("true"));
    }

    #[test]
    fn test_bool_neg() {
        assert_eq!(eval("let a = 5; in (-a) + 3"), eval("-2"));
    }

    #[test]
    fn test_func_call_laziness() {
        // The code contains an error; myfunc, which is not a function.
        // It is intentional that the func should not be evaluated, since
        // laziness in "false && ...", and therefore not be resolved as an
        // error.
        //
        // Test evalutes that eval is successful rather than ethe actual output
        assert_eq!(
            eval(
                r#"
                let
                    myfunc = not_a_function;
                    lazy_func_call = myfunc 72;
                in
                    false && lazy_func_call
                "#
            ),
            eval("false")
        );
    }

    #[derive(Debug, Clone)]
    struct CountingBuiltin(Rc<RefCell<i32>>);
    impl CountingBuiltin {
        fn new() -> CountingBuiltin {
            CountingBuiltin(Rc::new(RefCell::new(0i32)))
        }

        fn get(&self) -> i32 {
            *self.0.borrow()
        }
    }

    impl ExprBuiltin<TestValue> for CountingBuiltin {
        fn get_name(&self) -> String {
            "mybuiltin".into()
        }

        fn call(&self, arg: Expr<TestValue>) -> ops::Result<Expr<TestValue>> {
            let mut counter = self.0.borrow_mut();
            *counter += 1;
            Ok(arg)
        }
    }

    #[test]
    fn test_parse_func_call_from_obj() {
        assert_eq!(
            eval("let lib = { func = a: a+3; }; in (lib.func 7)"),
            eval("10")
        );
    }

    #[test]
    fn test_multi_level_obj() {
        assert_eq!(eval("let a = { b = { c = 3; }; }; in a.b.c"), eval("3"));
    }

    #[test]
    fn test_list_concat() {
        assert_eq!(
            eval("[1, 3, 5, 7] + [2, 4, 6, 8]"),
            eval("[1, 3, 5, 7, 2, 4, 6, 8]")
        );
    }

    #[test]
    fn test_list_commas() {
        assert_eq!(eval("[1, 3, 5, 7,]"), eval("[1, 3, 5, 7]"));
    }

    #[test]
    fn test_list_comprehension() {
        assert_eq!(eval("[ a: (a*2) <- [1, 2, 3] ]"), eval("[2, 4, 6]"));
    }

    #[test]
    fn test_div_precedence() {
        assert_eq!(eval("8 / 2 / 2"), eval("2"));
    }

    #[test]
    fn test_sub_precedence() {
        assert_eq!(eval("8 - 2 - 2"), eval("4"));
    }

    #[test]
    fn test_builtin_func() {
        let code = "mybuiltin 10";

        let builtins = ExprSet::from(vec![(
            "mybuiltin",
            Expr::new_builtin(Rc::new(CountingBuiltin::new())),
        )])
        .unwrap();
        let expr: Expr<TestValue> = BoundExpr(builtins, parse_str(code).unwrap()).into();
        expr.eval().unwrap();
        assert_eq!(expr, eval("10"));
    }

    #[test]
    fn test_builtin_func_laziness_multiple_calls() {
        // Invoked in code twice should only be evaluated once
        let code = r#"
                let
                    func_call = mybuiltin 10;
                in
                {
                    a = func_call;
                    b = func_call;
                }
            "#;
        let counter = CountingBuiltin::new();
        let builtins = ExprSet::from(vec![(
            "mybuiltin",
            Expr::new_builtin(Rc::new(counter.clone())),
        )])
        .unwrap();
        let expr: Expr<TestValue> = BoundExpr(builtins, parse_str(code).unwrap()).into();
        expr.eval().unwrap();
        assert_eq!(expr, eval("{ a = 10; b = 10; }"));
        assert_eq!(counter.get(), 1);
    }

    #[test]
    fn test_builtin_func_laziness_no_calls() {
        // Invoked in code twice should only be evaluated once
        let code = r#"
                let
                    func_call = mybuiltin 10;
                in
                {}
            "#;
        let counter = CountingBuiltin::new();
        let builtins = ExprSet::from(vec![(
            "mybuiltin",
            Expr::new_builtin(Rc::new(counter.clone())),
        )])
        .unwrap();
        let expr: Expr<TestValue> = BoundExpr(builtins, parse_str(code).unwrap()).into();
        expr.eval().unwrap();
        assert_eq!(expr, eval("{}"));
        assert_eq!(counter.get(), 0);
    }
}
