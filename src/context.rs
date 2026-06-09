use std::{
    fmt::{Debug, Display},
    fs, mem,
    path::PathBuf,
};

use crate::lang::{
    Expr, ExprError, ExprSet, ExprType, ParsableValue, Result, ops::ExprOps, parse_str,
};

pub struct LangContext<T>
where
    T: Clone + PartialEq + Display + ExprOps + ParsableValue + Debug,
{
    builtins: ExprSet<T>,
}

impl<T> LangContext<T>
where
    T: Clone + PartialEq + Display + ExprOps + ParsableValue + Debug,
{
    pub fn new() -> Self {
        LangContext {
            builtins: ExprSet::default(),
        }
    }

    //pub fn add_builtin<F>(&mut self, name: impl ToString, func: F) -> Result<()>
    //where
    //    F: 'static + Fn(&Expr<T>) -> std::result::Result<Expr<T>, ExprError>,
    //{
    //    let builtin_name = name.to_string();
    //    let builtin_expr = Expr::new_builtin(name, func);
    //    let previous = mem::replace(&mut self.builtins, ExprSet::new());
    //    self.builtins = previous.set(builtin_name, builtin_expr)?;
    //    Ok(())
    //}

    pub fn read_file(&self, filename: PathBuf) -> Result<Expr<T>> {
        let code = fs::read_to_string(filename).unwrap();
        //let builtin_include: Expr<T> = Expr::new_builtin("include", |expr| {
        //    let include_file = expr.eval_string()?;
        //    let include_expr = self.read_file(include_file.into())?;
        //    Ok(include_expr)
        //});
        //let builtins = self.builtins.clone().set("include", builtin_include)?;
        let builtins = self.builtins.clone();
        let expr: Expr<T> = ExprType::BoundExpr(builtins, parse_str(&code)?).into();
        Ok(expr)
    }
}
