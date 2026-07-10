use crate::lang::{Error, ErrorType, ExprStorage, ExprType, Result};
use crate::ninjawriter::NinjaFile;
use crate::path::VirtPath;
use crate::{Expr, Value};

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
            build.populate_ninja_file(ninja_file, true);
            Ok(())
        }
        ExprStorage {
            tok: ExprType::List(list),
            ..
        } => {
            for item in list.iter() {
                item.resolve()?;
                let build = item.value()?.try_as_build().ok_or_else(|| {
                    Error::new(ErrorType::Custom, "Top-level list contains non-build")
                        .reref(&item.get_loc())
                })?;
                build.populate_ninja_file(ninja_file, true);
            }
            Ok(())
        }
        ExprStorage {
            tok: ExprType::Object(fields),
            ..
        } => {
            for (name, value) in fields.iter() {
                value.resolve()?;
                let build = value.value()?.try_as_build().ok_or_else(|| {
                    Error::new(
                        ErrorType::Custom,
                        format!("Top-level field '{}' is not a build", name),
                    )
                    .reref(&value.get_loc())
                })?;

                build.populate_ninja_file(ninja_file, true);

                let alias = ninja_file.alias(name);
                for output in build.ninja_outputs().into_iter() {
                    alias.input(output);
                }
            }
            Ok(())
        }
        ExprStorage { tok: _, loc } => {
            Err(Error::new(ErrorType::Custom, "Not a valid build definition").reref(loc))
        }
    }
}
