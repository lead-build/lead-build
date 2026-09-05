use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    rc::Rc,
};

use crate::{
    Expr,
    pbexpr::{
        Error, ErrorType, ExprBuiltin, ExprSet, ExprStorage, ExprType, Matcher, Referrable, Result,
    },
    ninjawriter::NinjaArg,
    path::VirtPath,
    pbbuild::{PbBuild, PbBuildRule},
    strkey::StrKey,
    value::Value,
};

macro_rules! expr_get_arg (
    ($obj:expr, $name:expr, $unpack:ident) => {
        $obj
            .remove($name)
            .ok_or_else(|| Error::new(ErrorType::Type, format!("Can't unpack {}", stringify!($name))))?
            .value()?
            .$unpack()
            .ok_or_else(|| Error::new(ErrorType::Type, format!("Can't unpack {}", stringify!($name))))?
    };
    ($obj:expr, $name:expr) => {
        $obj
            .remove($name)
            .ok_or_else(|| Error::new(ErrorType::Type, format!("Can't unpack {}", stringify!($name))))?
    };
);

fn value_to_ninja_arg(attr: &Value) -> NinjaArg {
    match attr {
        Value::Int(value) => NinjaArg::Const(format!("{}", value)),
        Value::String(value) => NinjaArg::Const(value.clone()),
        Value::Path { path, .. } => NinjaArg::Path(path.clone()),
        Value::BuildVar(value) => NinjaArg::Var(value.as_string()),
        Value::BuildConcat(vs) => NinjaArg::Concat(
            vs.iter()
                .map(|v| match v {
                    Value::Int(value) => NinjaArg::Const(format!("{}", value)),
                    Value::String(value) => NinjaArg::Const(value.clone()),
                    Value::BuildVar(value) => NinjaArg::Var(value.as_string()),
                    Value::Path { path, .. } => NinjaArg::Path(path.clone()),
                    _ => unreachable!(),
                })
                .collect(),
        ),
        _ => panic!("Rule attr is of invalid type: {}", attr),
    }
}

enum BuildOutputFormat {
    Value,
    List,
    Object(Vec<StrKey>),
}

fn resolve_path_value_from_expr<F: Clone + Debug + Referrable>(
    expr: &Expr<Value, F>,
    field_name: &str,
    dep_builds: &mut Vec<Rc<PbBuild>>,
) -> Result<VirtPath, F> {
    expr.resolve()?;
    match &expr.inner_ref().tok {
        ExprType::Value(Value::Path { path, depends }) => {
            for dep_build in depends.iter() {
                dep_builds.push(dep_build.clone());
            }
            Ok(path.clone())
        }
        _ => Err(Error::new(
            ErrorType::Type,
            format!("incompatible type in build arg {}", field_name),
        )),
    }
}

fn resolve_build_arg_to_paths<F: Clone + Debug + Referrable>(
    build_arg: &Expr<Value, F>,
    field_name: &str,
    dep_builds: &mut Vec<Rc<PbBuild>>,
) -> Result<Vec<VirtPath>, F> {
    build_arg.resolve()?;
    let elems: Vec<Expr<Value, F>> = match &build_arg.inner_ref().tok {
        ExprType::Value(value) => vec![ExprType::from(value.clone()).reref(build_arg.get_loc())],
        ExprType::List(exprs) => exprs.clone(),
        ExprType::Object(fields) => fields.values().cloned().collect(),
        _ => Err(Error::new(
            ErrorType::Type,
            format!("field {} is not a value, list, or object", field_name),
        ))?,
    };

    elems
        .iter()
        .map(|expr| resolve_path_value_from_expr(expr, field_name, dep_builds))
        .collect()
}

fn resolve_build_arg_to_ninja_values<F: Clone + Debug + Referrable>(
    build_arg: &Expr<Value, F>,
    field_name: &str,
    dep_builds: &mut Vec<Rc<PbBuild>>,
) -> Result<Vec<NinjaArg>, F> {
    build_arg.resolve()?;
    let loc = build_arg.get_loc();

    let elems: Vec<Expr<Value, F>> = match &build_arg.inner_ref().tok {
        ExprType::List(exprs) => Ok(exprs.clone()),
        ExprType::Value(value) => Ok(vec![ExprType::from(value.clone()).reref(loc.clone())]),
        _ => Err(Error::new(
            ErrorType::Type,
            format!("field {} is not a list or value", field_name),
        )),
    }?;

    elems
        .into_iter()
        .map(|elem| {
            elem.resolve()?;
            match &elem.inner_ref().tok {
                ExprType::Value(attr) => {
                    if let Value::Path { depends, .. } = attr {
                        for dep_build in depends.iter() {
                            dep_builds.push(dep_build.clone());
                        }
                    }
                    Ok(value_to_ninja_arg(attr))
                }
                _ => Err(Error::new(
                    ErrorType::Type,
                    format!("incompatible type in build arg {}", field_name),
                )),
            }
        })
        .collect()
}

#[derive(Debug)]
pub struct BuiltinPbRule;

impl<F> ExprBuiltin<Value, F> for BuiltinPbRule
where
    F: Clone + Debug + Referrable,
{
    fn get_name(&self) -> StrKey {
        "build".into()
    }

    fn call(&self, arg: Expr<Value, F>) -> Result<Expr<Value, F>, F> {
        arg.resolve()?;
        let loc = arg.get_loc();

        let str_input = StrKey::from("input");
        let str_output = StrKey::from("output");

        /* Initialize meta variables, that may change later */
        let mut rule_args: BTreeSet<StrKey> = BTreeSet::new();

        /* Identify arguments */
        let match_items = match arg.inner_ref().tok.try_as_func_def_ref() {
            Some((Matcher::Object(items, _), _expr)) => Ok(items.clone()),
            _ => Err(Error::new(
                ErrorType::Type,
                "pb.rule needs to take a pattern function as argument",
            )),
        }?;

        /* Generate object with placeholders */
        let var_obj = ExprType::Object(
            match_items
                .iter()
                .map(|(name, _, default)| {
                    if default.is_some() {
                        return Err(Error::new(
                            ErrorType::Type,
                            format!("pb.rule does not support default values for {}", name),
                        ));
                    }

                    /* Also store names for validation from PbBuild */
                    rule_args.insert(*name);

                    /* Generate element */
                    Ok((
                        *name,
                        ExprType::from(Value::BuildVar(match name {
                            name if *name == str_input => StrKey::from("in"),
                            name if *name == str_output => StrKey::from("out"),
                            name => *name,
                        }))
                        .reref(loc.clone()),
                    ))
                })
                .collect::<Result<ExprSet<Value, F>, F>>()?,
        )
        .reref(loc.clone());

        /* Generate rule function with variable placeholders and call */
        let rule_func: Expr<Value, F> = ExprType::FuncCall {
            arg: var_obj,
            func: arg,
        }
        .reref(loc.clone());
        rule_func.resolve()?;

        /* Read variables */
        let objargs = match rule_func.inner_ref().tok.try_as_object_ref() {
            Some(args) => Ok(args.clone()),
            None => Err(Error::new(
                ErrorType::Type,
                format!(
                    "pb.rule function needs to return an object, got {}",
                    rule_func
                ),
            )),
        }?;

        /* Convert all variables to ninja rule */
        let mut vars: Vec<(StrKey, Vec<NinjaArg>)> = Vec::new();
        for (name, expr) in objargs.into_iter() {
            expr.resolve()?;
            let attrs = match &expr.inner_ref().tok {
                ExprType::List(exprs) => exprs.clone(),
                ExprType::Value(value) => vec![ExprType::from(value.clone()).reref(loc.clone())],
                _ => panic!("pb.rule function needs to return an object"),
            };
            let ninja_attrs: Vec<NinjaArg> = attrs
                .into_iter()
                .map(|e| {
                    e.resolve()?;
                    match &e.inner_ref().tok {
                        ExprType::Value(attr) => Ok(value_to_ninja_arg(attr)),
                        _ => Err(Error::new(ErrorType::Type, "Rule attr is not a value")),
                    }
                })
                .collect::<Result<Vec<NinjaArg>, _>>()?;

            vars.push((name, ninja_attrs));
        }

        /* Wrap into a node */
        Ok(
            ExprType::new_builtin(BuiltinPbBuild::new(PbBuildRule::new(rule_args, vars)))
                .reref(loc),
        )
    }
}

#[derive(Debug)]
pub struct BuiltinPbBuild(Rc<PbBuildRule>);

impl BuiltinPbBuild {
    pub fn new(rule: PbBuildRule) -> Self {
        BuiltinPbBuild(Rc::new(rule))
    }
}

impl<F> ExprBuiltin<Value, F> for BuiltinPbBuild
where
    F: Clone + Debug + Referrable,
{
    fn get_name(&self) -> StrKey {
        "build".into()
    }

    fn call(&self, arg: Expr<Value, F>) -> Result<Expr<Value, F>, F> {
        let BuiltinPbBuild(rule) = &self;

        arg.resolve()?;
        let loc = arg.get_loc();

        let opt_err = || {
            Error::new(
                ErrorType::Type,
                format!("unknown arg for pb.build, got {}", arg),
            )
        };

        /* Read arguments from input object */
        let mut arg_obj = arg
            .inner_ref()
            .clone()
            .tok
            .try_as_object()
            .ok_or_else(opt_err)?;

        /* Read all variables required by rule */
        let mut args: BTreeMap<StrKey, Vec<NinjaArg>> = BTreeMap::new();
        /* Special treatment for input/output */
        let mut input: Vec<VirtPath> = vec![];
        let mut output: Vec<VirtPath> = vec![];
        let mut output_format: Option<BuildOutputFormat> = None;
        /* Optional implicit deps for ninja build target */
        let mut deps: Vec<VirtPath> = vec![];
        /* Track all dependent rules, that needs to be added to ninja file  */
        let mut dep_builds: Vec<Rc<PbBuild>> = vec![];

        if !rule.rule_args().contains(&StrKey::from("deps"))
            && let Some(build_arg) = arg_obj.remove(&StrKey::from("deps"))
        {
            deps = resolve_build_arg_to_paths(&build_arg, "deps", &mut dep_builds)?;
        }

        for arg_name in rule.rule_args().iter() {
            /* Read variable */
            let build_arg = expr_get_arg!(arg_obj, arg_name);
            match arg_name.as_string().as_str() {
                "input" => {
                    input = resolve_build_arg_to_paths(&build_arg, "input", &mut dep_builds)?
                }
                "output" => {
                    build_arg.resolve()?;
                    let (output_exprs, output_format_type) = match &build_arg.inner_ref().tok {
                        ExprType::Value(value) => (
                            vec![ExprType::from(value.clone()).reref(build_arg.get_loc())],
                            BuildOutputFormat::Value,
                        ),
                        ExprType::List(exprs) => (exprs.clone(), BuildOutputFormat::List),
                        ExprType::Object(fields) => (
                            fields.values().cloned().collect(),
                            BuildOutputFormat::Object(fields.keys().cloned().collect()),
                        ),
                        _ => {
                            return Err(Error::new(
                                ErrorType::Type,
                                "output must be a value, list, or object",
                            )
                            .reref(&build_arg.get_loc()));
                        }
                    };
                    output_format = Some(output_format_type);
                    output = output_exprs
                        .into_iter()
                        .map(|expr| {
                            expr.resolve()?;
                            match &expr.inner_ref().tok {
                                ExprType::Value(Value::Path { path, .. }) => Ok(path.clone()),
                                _ => Err(Error::new(
                                    ErrorType::Type,
                                    format!(
                                        "incompatible type in build arg '{}' - {}",
                                        arg_name, expr
                                    ),
                                )),
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                }
                name => {
                    let value =
                        resolve_build_arg_to_ninja_values(&build_arg, name, &mut dep_builds)?;
                    args.insert(*arg_name, value);
                }
            }
        }

        let build = Rc::new(PbBuild::new(
            rule.clone(),
            input,
            output,
            deps,
            args,
            dep_builds,
        ));

        let output_format = output_format.ok_or_else(|| {
            Error::new(ErrorType::Type, "missing required output field").reref(&arg.get_loc())
        })?;

        match output_format {
            BuildOutputFormat::Value => {
                assert_eq!(build.ninja_outputs().len(), 1);
                Ok(ExprType::Value(Value::Path {
                    path: build.ninja_outputs()[0].clone(),
                    depends: vec![build],
                })
                .reref(loc))
            }
            BuildOutputFormat::List => {
                let out_list = build
                    .ninja_outputs()
                    .iter()
                    .map(|path| {
                        ExprType::Value(Value::Path {
                            path: path.clone(),
                            depends: vec![build.clone()],
                        })
                        .reref(loc.clone())
                    })
                    .collect();
                Ok(ExprType::List(out_list).reref(loc))
            }
            BuildOutputFormat::Object(field_names) => {
                assert_eq!(field_names.len(), build.ninja_outputs().len());
                let fields = field_names
                    .into_iter()
                    .zip(build.ninja_outputs().iter())
                    .map(|(name, path)| {
                        (
                            name,
                            ExprType::Value(Value::Path {
                                path: path.clone(),
                                depends: vec![build.clone()],
                            })
                            .reref(loc.clone()),
                        )
                    })
                    .collect::<ExprSet<Value, F>>();
                Ok(ExprType::Object(fields).reref(loc))
            }
        }
    }
}

#[derive(Debug)]
pub struct BuiltinPbLock;

impl ExprBuiltin<Value, VirtPath> for BuiltinPbLock {
    fn get_name(&self) -> StrKey {
        "lock".into()
    }

    fn call(&self, arg: Expr<Value, VirtPath>) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let val = arg.value()?;
        let path = val.try_as_path().ok_or(
            Error::new(ErrorType::Type, format!("expected path, got {}", arg))
                .reref(&arg.get_loc()),
        )?;
        Ok(ExprType::Value(Value::path(path.lock())).reref(arg.get_loc()))
    }
}

#[derive(Debug)]
pub struct BuiltinPbTranslate;

impl ExprBuiltin<Value, VirtPath> for BuiltinPbTranslate {
    fn get_name(&self) -> StrKey {
        "translate".into()
    }

    fn call(&self, arg: Expr<Value, VirtPath>) -> Result<Expr<Value, VirtPath>, VirtPath> {
        arg.resolve()?;
        let loc = arg.get_loc();

        let input = arg.get_item("input")?;
        let from = arg.get_item("from")?;
        let to = arg.get_item("to")?;
        // TODO: Verify no more args are available

        let input = input
            .value()
            .map_err(|e| e.reref(&input.get_loc()))?
            .try_as_path()
            .ok_or_else(|| Error::new(ErrorType::Type, "expected path").reref(&input.get_loc()))?;
        let to = to
            .value()
            .map_err(|e| e.reref(&to.get_loc()))?
            .try_as_path()
            .ok_or_else(|| Error::new(ErrorType::Type, "expected path").reref(&to.get_loc()))?;

        from.resolve()?;
        let from = match &*from.inner_ref() {
            ExprStorage {
                tok: ExprType::Value(val),
                ..
            } => vec![val.clone().try_as_path().ok_or_else(|| {
                Error::new(ErrorType::Type, "expected path").reref(&from.get_loc())
            })?],
            ExprStorage {
                tok: ExprType::List(vals),
                ..
            } => vals
                .iter()
                .map(|val| {
                    val.value()
                        .map_err(|e| e.reref(&val.get_loc()))?
                        .try_as_path()
                        .ok_or_else(|| {
                            Error::new(ErrorType::Type, "expected path").reref(&from.get_loc())
                        })
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                return Err(
                    Error::new(ErrorType::Type, "expected path or list of paths")
                        .reref(&from.get_loc()),
                );
            }
        };

        // Clone here only to allow error message

        let output_path = from
            .into_iter()
            .find_map(|from_path| input.clone().translate(&from_path, &to));

        let output = output_path.ok_or_else(|| {
            Error::new(ErrorType::Type, format!("Can't translate {}", input)).reref(&loc)
        })?;

        Ok(ExprType::Value(Value::path(output)).reref(loc))
    }
}

#[derive(Debug)]
pub struct BuiltinPbRebase;

impl ExprBuiltin<Value, VirtPath> for BuiltinPbRebase {
    fn get_name(&self) -> StrKey {
        "rebase".into()
    }

    fn call(&self, arg: Expr<Value, VirtPath>) -> Result<Expr<Value, VirtPath>, VirtPath> {
        arg.resolve()?;
        let loc = arg.get_loc();

        let path = arg.get_item("path")?;
        let base = arg.get_item("base")?;
        // TODO: Verify no more args are available

        let path = path
            .value()?
            .try_as_path()
            .ok_or_else(|| Error::new(ErrorType::Type, "expected path").reref(&path.get_loc()))?;
        let base = base
            .value()?
            .try_as_path()
            .ok_or_else(|| Error::new(ErrorType::Type, "expected path").reref(&base.get_loc()))?;

        // Clone here only to allow error message
        let output = path.to_path_buf_rebase(&base)?;

        // TODO: Handle this better than format!() and display()...
        // Push relative directories to NinjaArg::Path?
        Ok(ExprType::Value(Value::String(format!("{}", output.display()))).reref(loc))
    }
}

pub fn get_pb_builtins() -> Result<Expr<Value, VirtPath>, VirtPath> {
    let pbset = ExprSet::from([
        ("lock".into(), Expr::new_builtin(BuiltinPbLock)),
        ("rule".into(), Expr::new_builtin(BuiltinPbRule)),
        ("translate".into(), Expr::new_builtin(BuiltinPbTranslate)),
        ("rebase".into(), Expr::new_builtin(BuiltinPbRebase)),
    ]);
    Ok(ExprType::Object(pbset).builtin())
}
