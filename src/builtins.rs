mod dbg;
mod pb;

use crate::{
    Value,
    lang::{ExprSet, Result},
    path::VirtPath,
    strkey::StrKey,
};

pub fn get_builtins() -> Result<ExprSet<Value, VirtPath>, VirtPath> {
    let mut builtins = ExprSet::new();
    builtins.insert(StrKey::from("pb"), pb::get_pb_builtins()?);
    builtins.insert(StrKey::from("dbg"), dbg::get_dbg_builtins()?);
    Ok(builtins)
}
