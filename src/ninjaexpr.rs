use crate::{
    Expr, Value,
    lang::{Error, ErrorType, ExprStorage, ExprType, Result},
    ninjawriter::NinjaFile,
    path::VirtPath,
};

pub fn add_expr_to_ninjafile(
    expr: &Expr<Value, VirtPath>,
    ninja_file: &mut NinjaFile,
) -> Result<(), VirtPath> {
    add_expr_to_ninjafile_inner(expr, ninja_file, None)
}

fn add_expr_to_ninjafile_inner(
    expr: &Expr<Value, VirtPath>,
    ninja_file: &mut NinjaFile,
    alias: Option<&str>,
) -> Result<(), VirtPath> {
    expr.resolve()?;
    match &*expr.inner_ref() {
        ExprStorage {
            tok: ExprType::Value(value),
            ..
        } => {
            if let Value::Path { path, depends, .. } = value {
                if depends.is_empty() {
                    let message = alias.map_or_else(
                        || "Top-level value has no build dependencies".to_string(),
                        |alias| format!("Top-level field '{}' has no build dependencies", alias),
                    );
                    return Err(Error::new(ErrorType::Custom, message).reref(&expr.get_loc()));
                }
                for build in depends.iter() {
                    build.populate_ninja_file(ninja_file, true)?;
                }

                if let Some(alias) = alias {
                    ninja_file.add_alias(alias, vec![path.clone()]);
                }
            } else {
                let message = alias.map_or_else(
                    || "Top-level value is not a path".to_string(),
                    |alias| format!("Top-level field '{}' is not a path", alias),
                );
                return Err(Error::new(ErrorType::Custom, message).reref(&expr.get_loc()));
            }
            Ok(())
        }
        ExprStorage {
            tok: ExprType::List(list),
            ..
        } => {
            for item in list.iter() {
                add_expr_to_ninjafile_inner(item, ninja_file, alias)?;
            }
            Ok(())
        }
        ExprStorage {
            tok: ExprType::Object(fields),
            ..
        } => {
            for (name, value) in fields.iter() {
                let name = name.as_string();
                add_expr_to_ninjafile_inner(value, ninja_file, Some(&name))?;
            }
            Ok(())
        }
        ExprStorage { tok: _, loc } => {
            Err(Error::new(ErrorType::Custom, "Not a valid build definition").reref(loc))
        }
    }
}
