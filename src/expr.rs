use std::{
    fmt::{Debug, Display},
    rc::Rc,
};

use crate::immap::ImMap;

pub mod ops {
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
        fn from_bool(&self, value: bool) -> Self;
    }

    pub enum Error {
        Type(String),
    }

    pub type Result<T> = std::result::Result<T, Error>;
}

use ops::ExprOps;

/*
 * Error
 */

#[derive(Debug, PartialEq)]
pub enum Error<T: Clone + PartialEq + Display + ExprOps> {
    ScopeError(String, ExprSet<T>),
    EvalError(String),
    TypeError(String),
    DupKey(String),
}

impl<T> From<crate::immap::Error> for Error<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn from(value: crate::immap::Error) -> Self {
        match value {
            crate::immap::Error::DupKey(key) => Error::DupKey(key),
        }
    }
}

impl<T> From<ops::Error> for Error<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn from(value: ops::Error) -> Self {
        match value {
            ops::Error::Type(msg) => Error::TypeError(msg),
        }
    }
}

type Result<RT, ET> = std::result::Result<RT, Error<ET>>;

/*
 * Types
 */

pub type ExprSet<T> = ImMap<Expr<T>>;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ExprBinOp {
    AttrSel,
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

impl Display for ExprBinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExprBinOp::AttrSel => write!(f, "."),
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

#[derive(Debug, PartialEq)]
pub enum ExprType<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    Object(ExprSet<T>),
    Value(T),
    Var(String),
    UnOp(ExprUnOp, Expr<T>),
    BinOp(ExprBinOp, Expr<T>, Expr<T>),
    FuncDefIdent(String, Expr<T>),
    FuncDefPattern(Vec<String>, Expr<T>),
    Let(Vec<(String, Expr<T>)>, Expr<T>),
    FuncCall(String, Expr<T>),
    BoundExpr(ExprSet<T>, Expr<T>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct Expr<T>(Rc<ExprType<T>>)
where
    T: Clone + PartialEq + Display + ExprOps;

impl<T> From<ExprType<T>> for Expr<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn from(value: ExprType<T>) -> Self {
        Expr(value.into())
    }
}

impl<T> From<T> for Expr<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn from(value: T) -> Self {
        Expr(ExprType::Value(value).into())
    }
}

impl<T> Display for ExprType<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExprType::Object(im_map) => im_map.fmt(f),
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
            ExprType::FuncCall(name, expr) => write!(f, "{} {}", name, expr),
            ExprType::BoundExpr(scope, expr) => write!(f, "[ {} @ {} ]", scope, expr),
        }
    }
}

impl<T> Display for Expr<T>
where
    T: Clone + PartialEq + Display + ExprOps,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> Expr<T>
where
    T: Clone + PartialEq + Display + ExprOps + Debug,
{
    pub fn get_item(&self, item: &str) -> Option<Expr<T>> {
        match self.0.as_ref() {
            ExprType::Object(vars) => vars.get(item),
            _ => None,
        }
    }

    fn resolve_once(&self) -> Result<Expr<T>, T> {
        match self.0.as_ref() {
            ExprType::BoundExpr(varspace, bound_expr) => match bound_expr.0.as_ref() {
                ExprType::Object(fields) => Ok(ExprType::Object(
                    fields.map(|val| ExprType::BoundExpr(varspace.clone(), val.clone()).into()),
                )
                .into()),
                ExprType::Let(fields, target_expr) => {
                    let mut vars: ExprSet<T> = varspace.clone();
                    for (field_name, field_expr) in fields {
                        let field_vars = vars.clone();
                        vars = vars.set(
                            field_name.clone(),
                            ExprType::BoundExpr(field_vars, field_expr.clone()).into(),
                        )?;
                    }
                    Ok(ExprType::BoundExpr(vars, target_expr.clone()).into())
                }
                ExprType::FuncDefIdent(arg_name, func_expr) => {
                    let new_scope = varspace.clone().unset(arg_name.as_str());
                    Ok(ExprType::FuncDefIdent(
                        arg_name.clone(),
                        ExprType::BoundExpr(new_scope, func_expr.clone()).into(),
                    )
                    .into())
                }
                ExprType::FuncDefPattern(items, expr) => {
                    let mut new_scope = varspace.clone();
                    for item in items {
                        new_scope = new_scope.unset(item);
                    }
                    Ok(ExprType::FuncDefPattern(
                        items.clone(),
                        ExprType::BoundExpr(new_scope, expr.clone()).into(),
                    )
                    .into())
                }
                ExprType::Var(name) => match varspace.get(name) {
                    Some(value) => Ok(value),
                    None => Err(Error::ScopeError(
                        format!("Unknown variable {}", name),
                        varspace.clone(),
                    )),
                },
                ExprType::UnOp(op, expr) => {
                    Ok(ExprType::UnOp(*op, expr.clone().bind(varspace.clone())).into())
                }
                ExprType::BinOp(op, lhs, rhs) => Ok(ExprType::BinOp(
                    *op,
                    lhs.clone().bind(varspace.clone()),
                    rhs.clone().bind(varspace.clone()),
                )
                .into()),
                ExprType::FuncCall(func_name, arg_expr) => match varspace.get(func_name) {
                    Some(func) => {
                        let func = func.resolve()?; // TODO: wrong scope
                        let (args, func_expr) = match func.0.as_ref() {
                            ExprType::FuncDefIdent(arg_name, func_expr) => Ok((
                                ExprSet::single(
                                    arg_name.clone(),
                                    ExprType::BoundExpr(varspace.clone(), arg_expr.clone()).into(),
                                ),
                                func_expr,
                            )),
                            ExprType::FuncDefPattern(arg_names, func_expr) => {
                                let arg_expr = arg_expr.resolve()?;

                                let mut new_vars = ExprSet::new();
                                for arg_name in arg_names {
                                    let arg_value = match arg_expr.get_item(arg_name) {
                                        Some(x) => Ok(x),
                                        None => Err(Error::ScopeError(
                                            format!(
                                                "called {}, no attr {} found",
                                                func_name, arg_name
                                            ),
                                            varspace.clone(),
                                        )),
                                    }?;
                                    new_vars = new_vars.set(arg_name.clone(), arg_value)?;
                                }
                                Ok((new_vars, func_expr))
                            }
                            _ => Err(Error::ScopeError(
                                format!("called {}, which is a {}", func_name, func),
                                varspace.clone(),
                            )),
                        }?;

                        // If function contains a bound scope, it should still apply,
                        // and not overwrite input arguments.
                        match func_expr.0.as_ref() {
                            ExprType::BoundExpr(varspace, inner_expr) => Ok(ExprType::BoundExpr(
                                varspace.clone().merge(&args),
                                inner_expr.clone(),
                            )
                            .into()),
                            _ => Ok(ExprType::BoundExpr(args, func_expr.clone()).into()),
                        }
                    }
                    None => Err(Error::ScopeError(
                        format!("Unknown function name '{}'", func_name),
                        varspace.clone(),
                    )),
                },
                ExprType::Value(..) => Ok(bound_expr.clone()),
                ExprType::BoundExpr(inner_vars, inner_expr) => Ok(ExprType::BoundExpr(
                    varspace.clone().merge(inner_vars),
                    inner_expr.clone(),
                )
                .into()),
            },
            ExprType::UnOp(op, expr) => match op {
                ExprUnOp::Neg => match expr.resolve()?.0.as_ref() {
                    ExprType::Value(value) => Ok(value.op_neg()?.into()),
                    _ => Err(Error::EvalError(format!("negating non-value: {}", expr))),
                },
                ExprUnOp::Not => match expr.resolve()?.0.as_ref() {
                    ExprType::Value(value) => Ok(value.op_not()?.into()),
                    _ => Err(Error::EvalError(format!("negating non-value: {}", expr))),
                },
            },
            ExprType::BinOp(op, lhs, rhs) => match lhs.resolve()?.0.as_ref() {
                ExprType::Object(_lhs_obj) => todo!(),
                ExprType::Value(lhs_val) => match op {
                    ExprBinOp::LogAnd => match lhs_val.as_bool()? {
                        true => Ok(rhs.clone()),
                        false => Ok(lhs_val.from_bool(false).into()),
                    },
                    ExprBinOp::LogOr => match lhs_val.as_bool()? {
                        true => Ok(lhs_val.from_bool(true).into()),
                        false => Ok(rhs.clone()),
                    },
                    ExprBinOp::LogImpl => match lhs_val.as_bool()? {
                        false => Ok(lhs_val.from_bool(true).into()),
                        true => Ok(rhs.clone()),
                    },
                    _ => match rhs.resolve()?.0.as_ref() {
                        ExprType::Object(_rhs_obj) => todo!(),
                        ExprType::Value(rhs_val) => match op {
                            ExprBinOp::AttrSel => todo!(),
                            ExprBinOp::HasAttr => todo!(),
                            ExprBinOp::ListConcat => todo!(),
                            ExprBinOp::Mult => {
                                Ok(ExprType::Value(T::op_mult(lhs_val, rhs_val)?).into())
                            }
                            ExprBinOp::Div => {
                                Ok(ExprType::Value(T::op_div(lhs_val, rhs_val)?).into())
                            }
                            ExprBinOp::Sub => {
                                Ok(ExprType::Value(T::op_sub(lhs_val, rhs_val)?).into())
                            }
                            ExprBinOp::Add => {
                                Ok(ExprType::Value(T::op_add(lhs_val, rhs_val)?).into())
                            }
                            ExprBinOp::Update => todo!(),
                            ExprBinOp::Lt => {
                                Ok(ExprType::Value(T::op_lt(lhs_val, rhs_val)?).into())
                            }
                            ExprBinOp::Le => {
                                Ok(ExprType::Value(T::op_le(lhs_val, rhs_val)?).into())
                            }
                            ExprBinOp::Gt => {
                                Ok(ExprType::Value(T::op_gt(lhs_val, rhs_val)?).into())
                            }
                            ExprBinOp::Ge => {
                                Ok(ExprType::Value(T::op_ge(lhs_val, rhs_val)?).into())
                            }
                            ExprBinOp::Eq => {
                                Ok(ExprType::Value(T::op_eq(lhs_val, rhs_val)?).into())
                            }
                            ExprBinOp::Neq => {
                                Ok(ExprType::Value(T::op_neq(lhs_val, rhs_val)?).into())
                            }
                            _ => unreachable!(),
                        },
                        typ => Err(Error::EvalError(format!(
                            "Resolving unresolvable type {}",
                            typ
                        ))),
                    },
                },
                typ => Err(Error::EvalError(format!(
                    "Resolving unresolvable type {}",
                    typ
                ))),
            },
            _ => unreachable!(),
        }
    }

    pub fn resolve(&self) -> Result<Expr<T>, T> {
        let mut expr: Expr<T> = self.clone();
        while match expr.0.as_ref() {
            ExprType::Object(..) => false,
            ExprType::Value(..) => false,
            ExprType::Var(..) => true,
            ExprType::UnOp(..) => true,
            ExprType::BinOp(..) => true,
            ExprType::FuncDefIdent(..) => false,
            ExprType::FuncDefPattern(..) => false,
            ExprType::Let(..) => true,
            ExprType::FuncCall(..) => true,
            ExprType::BoundExpr(..) => true,
        } {
            expr = expr.resolve_once()?;
        }
        Ok(expr)
    }

    pub fn eval(&self) -> Result<Expr<T>, T> {
        let res = self.resolve()?;
        match res.0.as_ref() {
            ExprType::Object(fields) => {
                Ok(ExprType::Object(fields.map(|e| e.eval().unwrap())).into())
            }
            _ => Ok(res),
        }
    }

    pub fn bind(self, vars: ExprSet<T>) -> Expr<T> {
        ExprType::BoundExpr(vars, self).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_str;
    use crate::value::Value;

    fn eval(code: &str) -> Expr<Value> {
        parse_str(code)
            .unwrap()
            .bind(ExprSet::new())
            .eval()
            .unwrap()
    }

    #[test]
    fn test_resolve() {
        let expr: Expr<Value> = parse_str(
            r#"
                {
                    stuff = "hello";
                    something = "hej";
                }
            "#,
        )
        .unwrap();
        let value = expr.get_item("stuff").unwrap();
        assert_eq!(*value.0, ExprType::Value(Value::String("hello".into())));
    }

    #[test]
    fn test_resolve_deep() {
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
        let value = expr
            .get_item("something")
            .unwrap()
            .get_item("inner")
            .unwrap();
        assert_eq!(*value.0, ExprType::Value(Value::Int(55)));
    }

    #[test]
    fn test_let() {
        let value = parse_str(
            r#"
                let
                    a = 12;
                    b = 75;
                in
                b
            "#,
        )
        .unwrap()
        .bind(ExprSet::new())
        .resolve()
        .unwrap();
        assert_eq!(*value.0, ExprType::Value(Value::Int(75)));
    }

    #[test]
    fn test_invalid_var() {
        let expr: Expr<Value> = parse_str(
            r#"
                invalid_var
            "#,
        )
        .unwrap()
        .bind(ExprSet::new());
        if let Error::ScopeError(message, _) = expr.resolve().unwrap_err() {
            assert_eq!(message.as_str(), "Unknown variable invalid_var");
        } else {
            assert!(false);
        }
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
        let varscope = ExprSet::from(vec![("func_a", func_a), ("func_b", func_b)]).unwrap();
        let value: Expr<Value> = call.bind(varscope).resolve().unwrap();
        assert_eq!(*value.0, ExprType::Value(Value::Int(42)));
    }

    #[test]
    fn test_func_call_var_arg() {
        let func_var = parse_str("var: var").unwrap();
        let arg_var = parse_str("32").unwrap();
        let call = parse_str("func arg").unwrap();
        let varscope = ExprSet::from(vec![("func", func_var), ("arg", arg_var)]).unwrap();
        let value: Expr<Value> = call.bind(varscope).resolve().unwrap();
        assert_eq!(*value.0, ExprType::Value(Value::Int(32)));
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
}
