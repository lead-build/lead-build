mod dbg;
mod ops;
mod pb;

use crate::{
    Value,
    pbexpr::{ExprSet, Result},
    path::VirtPath,
    strkey::StrKey,
};

pub fn get_builtins() -> Result<ExprSet<Value, VirtPath>, VirtPath> {
    let mut builtins = ExprSet::new();
    builtins.insert(StrKey::from("pb"), pb::get_pb_builtins()?);
    builtins.insert(StrKey::from("ops"), ops::get_ops_builtins()?);
    builtins.insert(StrKey::from("dbg"), dbg::get_dbg_builtins()?);
    Ok(builtins)
}
