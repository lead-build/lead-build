use std::fmt::Debug;

use crate::{
    Expr,
    lang::{Error, ErrorType, ExprBuiltin, ExprSet, ExprType, Result},
    path::VirtPath,
    strkey::StrKey,
    value::Value,
};

#[derive(Debug)]
pub struct BuiltinDbgTrace;

impl ExprBuiltin<Value, VirtPath> for BuiltinDbgTrace {
    fn get_name(&self) -> StrKey {
        "trace".into()
    }

    fn call(&self, arg: Expr<Value, VirtPath>) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let _ = arg.eval();
        println!("{}", arg);
        Ok(arg)
    }
}

#[derive(Debug)]
pub struct BuiltinDbgBreak;

impl ExprBuiltin<Value, VirtPath> for BuiltinDbgBreak {
    fn get_name(&self) -> StrKey {
        "break".into()
    }

    fn call(&self, arg: Expr<Value, VirtPath>) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let _ = arg.eval();
        println!("{}", arg);
        Err(Error::new(ErrorType::Debug, "break").reref(&arg.get_loc()))
    }
}

pub fn get_dbg_builtins() -> Result<Expr<Value, VirtPath>, VirtPath> {
    let dbgset = ExprSet::from([
        (StrKey::from("trace"), Expr::new_builtin(BuiltinDbgTrace)),
        (StrKey::from("break"), Expr::new_builtin(BuiltinDbgBreak)),
    ]);
    Ok(ExprType::Object(dbgset).builtin())
}
