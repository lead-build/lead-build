use std::{fmt::Debug, fs, rc::Rc};

use crate::{
    lang::{Error, ErrorType, Expr, ExprBuiltin, ExprSet, ExprType, Result, parse_str},
    path::VirtPath,
    pbbuild::get_pb_builtins,
    value::Value,
};

/*
 * Core builtins: include and lock
 */

#[derive(Debug)]
struct BuiltinInclude(LangContext);

impl ExprBuiltin<Value, VirtPath> for BuiltinInclude {
    fn get_name(&self) -> String {
        "include".into()
    }

    fn call(&self, arg: Expr<Value, VirtPath>) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let file_value = arg.value()?;
        let file_path = file_value
            .try_as_path()
            .ok_or(Error::new(ErrorType::Type, "Include of non-path argument"))?;
        let result = self.0.include(file_path)?;
        Ok(result)
    }
}

#[derive(Debug)]
struct LangContextStorage {
    builtins: ExprSet<Value, VirtPath>,
}

#[derive(Debug, Clone)]
pub struct LangContext(Rc<LangContextStorage>);

impl Default for LangContext {
    fn default() -> Self {
        let mut builtins = ExprSet::new();
        builtins.insert("pb".into(), get_pb_builtins().unwrap());
        LangContext(Rc::new(LangContextStorage { builtins }))
    }
}

impl LangContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_builtin(&mut self, name: impl ToString, value: Expr<Value, VirtPath>) {
        Rc::get_mut(&mut self.0)
            .unwrap()
            .builtins
            .insert(name.to_string(), value);
    }

    fn setup_file_args(&self, file: VirtPath) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let cwd = file.parent().unwrap().lock();
        let mut builtins = self.0.builtins.clone();
        builtins.insert("cwd".into(), ExprType::from(Value::Path(cwd)).builtin());
        builtins.insert(
            "include".to_string(),
            Expr::new_builtin(Rc::new(BuiltinInclude(self.clone()))),
        );
        Ok(ExprType::from(builtins).builtin())
    }

    pub fn read_file(&self, filename: &VirtPath) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let fs_path = filename.to_path_buf();
        let code = fs::read_to_string(fs_path.clone()).map_err(|_| {
            Error::new(
                ErrorType::Custom,
                format!("File not found: {}", fs_path.display()),
            )
        })?;
        let expr: Expr<Value, VirtPath> = parse_str(&code, filename)?;
        Ok(expr)
    }

    pub fn include(&self, file: VirtPath) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let file_expr = self.read_file(&file)?;
        let file_args = self.setup_file_args(file)?;
        let called_expr: Expr<Value, VirtPath> = ExprType::FuncCall(file_args, file_expr).builtin(); // TODO: Should this outermost builtin actually be a .loc()?
        Ok(called_expr)
    }
}
