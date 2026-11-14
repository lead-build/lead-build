use std::{fmt::Debug, fs, rc::Rc};

use crate::{
    pbexpr::{Error, ErrorType, Expr, ExprBuiltin, ExprSet, ExprType, Loc, Result, parse_str},
    strkey::StrKey,
};

use super::{builtins::get_builtins, path::VirtPath, value::Value};

/*
 * Core builtins: include
 */

#[derive(Debug)]
struct BuiltinInclude(LangContext);

impl ExprBuiltin<Value, VirtPath> for BuiltinInclude {
    fn get_name(&self) -> StrKey {
        "include".into()
    }

    fn call(&self, arg: Expr<Value, VirtPath>) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let file_value = arg.value()?;
        let file_path = file_value.try_as_path().ok_or(Error::new(
            ErrorType::Type,
            "Expected path as argument of `include`",
        ))?;
        // The argument is where the file is included from
        let result = self.0.include_at(file_path, None, arg.get_loc())?;
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
        let builtins = get_builtins().unwrap_or_else(|e| {
            panic!("[internal error]: Failed to initialize the builtins: {}", e);
        });
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
            .insert(StrKey::from(&name.to_string()), value);
    }

    fn setup_file_args(
        &self,
        file: VirtPath,
        args: Option<Expr<Value, VirtPath>>,
    ) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let cwd = file.parent()?.lock();
        let mut builtins = self.0.builtins.clone();
        builtins.insert(
            StrKey::from("cwd"),
            ExprType::from(Value::path(cwd)).builtin(),
        );
        builtins.insert(
            StrKey::from("include"),
            Expr::new_builtin(BuiltinInclude(self.clone())),
        );
        if let Some(args) = args {
            builtins.insert(StrKey::from("args"), args);
        }
        Ok(ExprType::from(builtins).builtin())
    }

    pub fn read_file(&self, filename: &VirtPath) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let fs_path = filename.to_path_buf();
        let code = fs::read_to_string(fs_path.clone()).map_err(|e| {
            Error::new(
                ErrorType::Io,
                format!("Failed to read `{}`: {}", fs_path.display(), e),
            )
        })?;
        let expr: Expr<Value, VirtPath> = parse_str(&code, filename)?;
        Ok(expr)
    }

    pub fn include(
        &self,
        file: VirtPath,
        args: Option<Expr<Value, VirtPath>>,
    ) -> Result<Expr<Value, VirtPath>, VirtPath> {
        self.include_at(file, args, None)
    }

    /// Include a file from `call_site`, the place in another file that
    /// asks for it. An error in the included file then shows where it was
    /// included from.
    fn include_at(
        &self,
        file: VirtPath,
        args: Option<Expr<Value, VirtPath>>,
        call_site: Option<Loc<VirtPath>>,
    ) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let file_expr = self.read_file(&file)?;
        let file_args = self.setup_file_args(file, args)?;
        let called_expr: Expr<Value, VirtPath> = ExprType::FuncCall {
            arg: file_args,
            func: file_expr,
        }
        .reref(call_site);
        Ok(called_expr)
    }
}
