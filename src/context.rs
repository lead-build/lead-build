use std::{fmt::Debug, fs, path::Path, rc::Rc};

use crate::{
    lang::{Expr, ExprSet, ExprType, Result, ops::ExprBuiltin, parse_str},
    path::VirtPath,
    pbbuild::get_pb_builtins,
    value::Value,
};

/*
 * Core builtins: include and lock
 */

#[derive(Debug)]
struct BuiltinInclude(LangContext);

impl ExprBuiltin<Value> for BuiltinInclude {
    fn get_name(&self) -> String {
        "include".into()
    }

    fn call(&self, arg: Expr<Value>) -> crate::lang::ops::Result<Expr<Value>> {
        let file_value = arg.value()?;
        let file_path = file_value
            .try_as_path()
            .ok_or(crate::lang::ops::Error::Type(
                "Include of non-path argument".into(),
            ))?;
        let result = self.0.include(file_path)?;
        Ok(result)
    }
}

#[derive(Debug)]
pub struct BuiltinLock;

impl ExprBuiltin<Value> for BuiltinLock {
    fn get_name(&self) -> String {
        "lock".into()
    }

    fn call(
        &self,
        arg: crate::lang::Expr<Value>,
    ) -> crate::lang::ops::Result<crate::lang::Expr<Value>> {
        let val = arg.value()?;
        let path = val
            .try_as_path()
            .ok_or(crate::lang::ops::Error::Type(format!(
                "expected path, got {}",
                arg
            )))?;
        Ok(ExprType::Value(Value::Path(path.lock())).into())
    }
}

#[derive(Debug)]
struct LangContextStorage {
    builtins: ExprSet<Value>,
}

#[derive(Debug, Clone)]
pub struct LangContext(Rc<LangContextStorage>);

impl Default for LangContext {
    fn default() -> Self {
        let mut builtins = ExprSet::new();
        builtins.insert("lock".to_string(), Expr::new_builtin(Rc::new(BuiltinLock)));
        builtins.insert("pb".to_string(), get_pb_builtins().unwrap());
        LangContext(Rc::new(LangContextStorage { builtins }))
    }
}

impl LangContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_builtin(&mut self, name: impl ToString, value: Expr<Value>) {
        Rc::get_mut(&mut self.0)
            .unwrap()
            .builtins
            .insert(name.to_string(), value);
    }

    fn setup_file_args(&self, file: VirtPath) -> Result<Expr<Value>> {
        let cwd = file.parent().unwrap().lock();

        Ok(ExprSet::from([("cwd".to_string(), Expr::from(Value::Path(cwd)))]).into())
    }

    fn setup_file_builtins(&self) -> Result<ExprSet<Value>> {
        let storage = self.0.as_ref();
        let mut builtins = storage.builtins.clone();
        builtins.insert(
            "include".to_string(),
            Expr::new_builtin(Rc::new(BuiltinInclude(self.clone()))),
        );
        Ok(builtins)
    }

    pub fn read_file(&self, filename: &Path) -> Result<Expr<Value>> {
        let code = fs::read_to_string(filename).unwrap();
        let expr: Expr<Value> = parse_str(&code)?;
        Ok(expr)
    }

    pub fn include(&self, file: VirtPath) -> Result<Expr<Value>> {
        let fs_path = file.to_path_buf();
        let file_expr = self.read_file(&fs_path)?;
        let file_args = self.setup_file_args(file)?;
        let file_builtins = self.setup_file_builtins()?;
        let called_expr: Expr<Value> = ExprType::FuncCall(
            ExprType::BoundExpr(file_builtins, file_expr).into(),
            file_args,
        )
        .into();
        Ok(called_expr)
    }
}
