use crate::{Expr, Value};
use crate::lang::{Error, ErrorType, ExprStorage, ExprType, Result};
use crate::ninjawriter::NinjaFile;
use crate::path::VirtPath;

pub fn add_expr_to_ninjafile(
    expr: &Expr<Value, VirtPath>,
    ninja_file: &mut NinjaFile,
) -> Result<(), VirtPath> {
    expr.resolve()?;
    match &*expr.inner_ref() {
        ExprStorage {
            tok: ExprType::Value(Value::Build(build)),
            ..
        } => {
            build.populate_ninja_file(ninja_file);
            Ok(())
        }
        ExprStorage {
            tok: ExprType::List(list),
            ..
        } => {
            for item in list.iter() {
                add_expr_to_ninjafile(item, ninja_file)?;
            }
            Ok(())
        }
        ExprStorage { tok: _, loc } => {
            Err(Error::new(ErrorType::Custom, "Not a valid build definition").reref(loc))
        }
    }
}
