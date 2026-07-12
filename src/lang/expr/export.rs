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

fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !first.is_ascii_alphabetic() {
        return false;
    }

    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
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
            ExprType::Concat(items) => {
                for (idx, item) in items.iter().enumerate() {
                    if idx != 0 {
                        write!(f, " + ")?;
                    }
                    item.export(indent, f)?;
                }
                Ok(())
            }
            ExprType::AttrSel(val, attr) => {
                val.export(indent, f)?;

                // Dot-identifier syntax is parsed into a string value; print it unquoted
                // when it is a plain identifier so pretty output matches source style.
                if let ExprType::Value(attr_value) = &attr.inner_ref().tok
                    && let Ok(name) = attr_value.as_string()
                    && is_ident(name.as_str())
                {
                    write!(f, ".{}", name)?;
                    return Ok(());
                }

                write!(f, ".{{")?;
                attr.export(indent + 1, f)?;
                write!(f, "}}")?;
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
            ExprType::FuncDef(matcher, expr) => {
                f.write_str("|")?;
                matcher.export(indent, f)?;
                f.write_str("| ")?;
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
            ExprType::Fold(func, init, input) => {
                write!(f, "( ")?;
                newline(indent + 1, f)?;
                func.export(indent + 1, f)?;
                newline(indent, f)?;
                write!(f, " for ")?;
                newline(indent + 1, f)?;
                init.export(indent + 1, f)?;
                write!(f, " | ")?;
                input.export(indent + 1, f)?;
                newline(indent, f)?;
                write!(f, " )")?;
                Ok(())
            }
            ExprType::Map(typ, func, input, filter) => {
                match typ {
                    ExprMapType::List => write!(f, "[ ")?,
                    ExprMapType::Object => write!(f, "{{ ")?,
                };
                newline(indent + 1, f)?;
                func.export(indent + 1, f)?;
                newline(indent, f)?;
                write!(f, " for ")?;
                newline(indent + 1, f)?;
                input.export(indent + 1, f)?;
                newline(indent, f)?;
                if let Some(filter_expr) = filter {
                    write!(f, " if ")?;
                    newline(indent + 1, f)?;
                    filter_expr.export(indent + 1, f)?;
                    newline(indent, f)?;
                }
                match typ {
                    ExprMapType::List => write!(f, " ]")?,
                    ExprMapType::Object => write!(f, " }}")?,
                };
                Ok(())
            }
            ExprType::FuncCall(farg, fexpr) => {
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
            ExprType::Switch(expr, cases, default) => {
                write!(f, "match ")?;
                expr.export(indent + 1, f)?;
                write!(f, " {{")?;
                for (matcher, case_expr) in cases {
                    newline(indent + 2, f)?;
                    matcher.export(indent + 2, f)?;
                    write!(f, " => ")?;
                    case_expr.export(indent + 2, f)?;
                    write!(f, ";")?;
                }
                if let Some(default_expr) = default {
                    newline(indent + 2, f)?;
                    write!(f, "_ => ")?;
                    default_expr.export(indent + 2, f)?;
                    write!(f, ";")?;
                }
                newline(indent + 1, f)?;
                write!(f, "}}")?;
                Ok(())
            }
            ExprType::FuncDefBuiltin(ExprBuiltinWrapper(name, _)) => {
                write!(f, "<builtin {}>", name)
            }
            ExprType::Null => write!(f, "null"),
        }
    }
}
