use std::{fmt::Debug, rc::Rc};

use crate::{
    Expr,
    lang::{Error, ErrorType, ExprBuiltin, ExprSet, ExprType, Result},
    path::VirtPath,
    value::Value,
};

#[derive(Debug)]
pub struct BuiltinDbgTrace;

impl ExprBuiltin<Value, VirtPath> for BuiltinDbgTrace {
    fn get_name(&self) -> String {
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
    fn get_name(&self) -> String {
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
        ("trace".into(), Expr::new_builtin(Rc::new(BuiltinDbgTrace))),
        ("break".into(), Expr::new_builtin(Rc::new(BuiltinDbgBreak))),
    ]);
    Ok(ExprType::Object(dbgset).builtin())
}
