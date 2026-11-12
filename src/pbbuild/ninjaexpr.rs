use crate::{
    Expr, Value,
    pbexpr::{Error, ErrorList, ErrorType, ExprStorage, ExprType, Result},
};

use super::{build::PopulatedBuilds, ninjawriter::NinjaFile, path::VirtPath};

pub fn add_expr_to_ninjafile(
    expr: &Expr<Value, VirtPath>,
    ninja_file: &mut NinjaFile,
) -> Result<(), VirtPath> {
    add_expr_to_ninjafile_inner(expr, ninja_file, &mut PopulatedBuilds::default(), None)
}

/// Like [`add_expr_to_ninjafile`], but with the errors of all the parts of
/// the result that fail, instead of stopping at the first one. With any
/// error, the ninja file is incomplete and must not be written.
pub fn add_expr_to_ninjafile_all(
    expr: &Expr<Value, VirtPath>,
    ninja_file: &mut NinjaFile,
) -> ErrorList<VirtPath> {
    let mut errors = ErrorList::new();
    add_expr_to_ninjafile_collect(
        expr,
        ninja_file,
        &mut PopulatedBuilds::default(),
        None,
        &mut Vec::new(),
        &mut errors,
    );
    errors
}

/// The part at `path` in the result (outermost part first) for
/// [`add_expr_to_ninjafile_all`]: lists and objects are walked here, each
/// item or field on its own; anything else is added as one.
fn add_expr_to_ninjafile_collect(
    expr: &Expr<Value, VirtPath>,
    ninja_file: &mut NinjaFile,
    populated: &mut PopulatedBuilds,
    alias: Option<&str>,
    path: &mut Vec<String>,
    errors: &mut ErrorList<VirtPath>,
) {
    // `in_result` adds the innermost part first
    let at_path = |err: Error<VirtPath>, path: &[String]| {
        path.iter().rev().fold(err, |err, part| err.in_result(part))
    };
    if let Err(err) = expr.resolve() {
        errors.push(at_path(err, path));
        return;
    }
    // The parts, each with its place in the result and the alias it gets
    type Part = (String, Option<String>, Expr<Value, VirtPath>);
    let parts: Option<Vec<Part>> =
        match &expr.inner_ref().tok {
            ExprType::List(items) => Some(
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| (format!("[{index}]"), alias.map(str::to_string), item.clone()))
                    .collect(),
            ),
            ExprType::Object(fields) => Some(
                fields
                    .iter()
                    .map(|(name, value)| (format!(".{name}"), Some(name.as_string()), value.clone()))
                    .collect(),
            ),
            _ => None,
        };
    match parts {
        Some(parts) => {
            for (part, alias, value) in parts {
                if errors.is_full() {
                    return;
                }
                path.push(part);
                add_expr_to_ninjafile_collect(
                    &value,
                    ninja_file,
                    populated,
                    alias.as_deref(),
                    path,
                    errors,
                );
                path.pop();
            }
        }
        None => {
            if let Err(err) = add_expr_to_ninjafile_inner(expr, ninja_file, populated, alias) {
                errors.push(at_path(err, path));
            }
        }
    }
}

fn add_expr_to_ninjafile_inner(
    expr: &Expr<Value, VirtPath>,
    ninja_file: &mut NinjaFile,
    populated: &mut PopulatedBuilds,
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
                    build.populate_ninja_file(ninja_file, populated, true)?;
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
            for (index, item) in list.iter().enumerate() {
                add_expr_to_ninjafile_inner(item, ninja_file, populated, alias)
                    .map_err(|e| e.in_result(format!("[{index}]")))?;
            }
            Ok(())
        }
        ExprStorage {
            tok: ExprType::Object(fields),
            ..
        } => {
            for (name, value) in fields.iter() {
                let name = name.as_string();
                add_expr_to_ninjafile_inner(value, ninja_file, populated, Some(&name))
                    .map_err(|e| e.in_result(format!(".{name}")))?;
            }
            Ok(())
        }
        ExprStorage { tok: _, loc } => {
            Err(Error::new(ErrorType::Custom, "Not a valid build definition").reref(loc))
        }
    }
}
