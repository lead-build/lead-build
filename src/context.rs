use std::{
    collections::BTreeMap,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

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
    path_refs: BTreeMap<String, PathBuf>,
    builtins: ExprSet<Value>,
}

#[derive(Debug, Clone)]
pub struct LangContext(Rc<LangContextStorage>);

impl Default for LangContext {
    fn default() -> Self {
        let builtins = ExprSet::new()
            .set("lock", Expr::new_builtin(Rc::new(BuiltinLock)))
            .unwrap()
            .set("pb", get_pb_builtins().unwrap())
            .unwrap();
        LangContext(Rc::new(LangContextStorage {
            path_refs: Default::default(),
            builtins,
        }))
    }
}

impl LangContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_path_ref(&mut self, name: impl ToString, path: &Path) {
        Rc::get_mut(&mut self.0)
            .unwrap()
            .path_refs
            .insert(name.to_string(), path.to_path_buf());
    }

    pub fn add_builtin(&mut self, name: impl ToString, value: Expr<Value>) {
        Rc::get_mut(&mut self.0)
            .unwrap()
            .builtins
            .set_mut(name, value)
            .unwrap();
    }

    pub fn virtualize_path(&mut self, root: impl ToString, path: &Path) -> Result<VirtPath> {
        let path_refs = &mut Rc::get_mut(&mut self.0).unwrap().path_refs;
        let virtpath = VirtPath::virtualize(path, root, path_refs);
        match virtpath {
            Some(path) => Ok(path),
            None => Err(format!("Can't virtualize path {}", path.display()).into()),
        }
    }

    fn setup_file_args(&self, file: VirtPath) -> Result<Expr<Value>> {
        let cwd = file.parent().unwrap().lock();

        Ok(ExprSet::from(vec![("cwd", Value::Path(cwd).into())])?.into())
    }

    fn setup_file_builtins(&self) -> Result<ExprSet<Value>> {
        let storage = self.0.as_ref();
        let mut builtins = storage.builtins.clone();
        builtins
            .set_mut(
                "include",
                Expr::new_builtin(Rc::new(BuiltinInclude(self.clone()))),
            )
            .unwrap();
        Ok(builtins)
    }

    pub fn read_file(&self, filename: &Path) -> Result<Expr<Value>> {
        let code = fs::read_to_string(filename).unwrap();
        let expr: Expr<Value> = parse_str(&code)?;
        Ok(expr)
    }

    pub fn include(&self, file: VirtPath) -> Result<Expr<Value>> {
        let storage = self.0.as_ref();
        let fs_path = file.to_path_buf(&storage.path_refs).unwrap();
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
