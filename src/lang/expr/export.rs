use std::fmt::Write;

use super::*;

fn indent(lvl: i32, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    for _ in 0..lvl {
        f.write_str("  ")?
    }
    Ok(())
}

fn newline(lvl: i32, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_char('\n')?;
    indent(lvl, f)?;
    Ok(())
}

pub trait Exportable {
    fn export(&self, indent: i32, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;
}

impl<T, F> Exportable for super::Expr<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug,
{
    fn export(&self, indent: i32, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner_ref().tok.export(indent, f)
    }
}

impl<T, F> Exportable for super::ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug,
{
    fn export(&self, indent: i32, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExprType::Object(varscope) => {
                write!(f, "{{")?;
                for (key, value) in varscope.iter() {
                    newline(indent + 1, f)?;
                    write!(f, "{} = ", key)?;
                    value.export(indent + 1, f)?;
                    write!(f, ";")?;
                }
                newline(indent, f)?;
                write!(f, "}}")?;
                Ok(())
            }
            ExprType::List(items) => {
                write!(f, "[")?;
                for item in items.iter() {
                    newline(indent + 1, f)?;
                    item.export(indent + 1, f)?;
                    write!(f, ",")?;
                }
                newline(indent, f)?;
                write!(f, "]")?;
                Ok(())
            }
            ExprType::Tuple(items) => {
                write!(f, "(")?;
                for item in items.iter() {
                    newline(indent + 1, f)?;
                    item.export(indent + 1, f)?;
                    write!(f, ",")?;
                }
                newline(indent, f)?;
                write!(f, ")")?;
                Ok(())
            }
            ExprType::AttrSel(val, attr) => {
                val.export(indent, f)?;
                write!(f, ".{}", attr)?;
                Ok(())
            }
            ExprType::Value(val) => val.export(indent, f),
            ExprType::Var(val) => Display::fmt(&val, f),
            ExprType::UnOp(op, expr) => {
                write!(f, "{}(", op)?;
                expr.export(indent, f)?;
                write!(f, ")")?;
                Ok(())
            }
            ExprType::BinOp(op, lhs, rhs) => {
                write!(f, "(")?;
                lhs.export(indent, f)?;
                write!(f, "){}(", op)?;
                rhs.export(indent, f)?;
                write!(f, ")")?;
                Ok(())
            }
            ExprType::FuncDefIdent(name, expr) => {
                write!(f, "{}: ", name)?;
                expr.export(indent, f)?;
                Ok(())
            }
            ExprType::FuncDefPattern(items, expr) => {
                f.write_str("{")?;
                for item in items {
                    Display::fmt(&item, f)?;
                    f.write_str(", ")?;
                }
                f.write_str("...}: ")?;
                expr.export(indent, f)?;
                Ok(())
            }
            ExprType::Let(items, expr) => {
                write!(f, "let")?;
                for (var_name, var_expr) in items {
                    newline(indent + 1, f)?;
                    write!(f, "{} = ", var_name)?;
                    var_expr.export(indent + 1, f)?;
                    write!(f, ";")?;
                }
                newline(indent, f)?;
                write!(f, "in")?;
                newline(indent + 1, f)?;
                expr.export(indent + 1, f)?;
                Ok(())
            }
            ExprType::MapList(func, input) => {
                write!(f, "[ ")?;
                newline(indent + 1, f)?;
                func.export(indent + 1, f)?;
                newline(indent, f)?;
                write!(f, " <- ")?;
                newline(indent + 1, f)?;
                input.export(indent + 1, f)?;
                newline(indent, f)?;
                write!(f, " ]")?;
                Ok(())
            }
            ExprType::FuncCall(fexpr, farg) => {
                write!(f, "(")?;
                newline(indent + 1, f)?;
                fexpr.export(indent + 1, f)?;
                newline(indent, f)?;
                write!(f, ") (")?;
                newline(indent + 1, f)?;
                farg.export(indent + 1, f)?;
                newline(indent, f)?;
                write!(f, ")")?;
                Ok(())
            }
            ExprType::Bind(scope, expr) => {
                write!(f, "bind")?;
                for (var_name, var_expr) in scope.iter() {
                    newline(indent + 1, f)?;
                    write!(f, "{} = ", var_name)?;
                    var_expr.export(indent + 1, f)?;
                    write!(f, ";")?;
                }
                newline(indent, f)?;
                write!(f, "in")?;
                newline(indent + 1, f)?;
                expr.export(indent + 1, f)?;
                Ok(())
            }
            ExprType::FuncDefBuiltin(ExprBuiltinWrapper(name, _)) => {
                write!(f, "<builtin {}>", name)
            }
            ExprType::Null => write!(f, "null"),
        }
    }
}
