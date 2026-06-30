use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Debug, Display},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    Expr,
    lang::{Error, ErrorType, ExprBuiltin, ExprSet, ExprType, Matcher, Result},
    ninjawriter::{NinjaArg, NinjaFile, NinjaRuleRef},
    path::VirtPath,
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

/*
 * Generate unique ID
 */

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

fn unique_id() -> usize {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/*
 * Build
 */

#[derive(PartialEq, Debug)]
pub struct PbBuildRule {
    id: usize,
    name: String,
    rule_args: BTreeSet<String>,
    rule_vars: Vec<(String, Vec<NinjaArg>)>,
}

impl Display for PbBuildRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BuildRule({})", self.name)
    }
}

impl PbBuildRule {
    fn new(rule_args: BTreeSet<String>, rule_vars: Vec<(String, Vec<NinjaArg>)>) -> Self {
        PbBuildRule {
            id: unique_id(),
            name: Self::get_name(&rule_vars),
            rule_args,
            rule_vars,
        }
    }

    fn get_name(rule_vars: &[(String, Vec<NinjaArg>)]) -> String {
        // Generate a descriptive name
        //
        // This name should be somewhat unique and descriptive, to simplify
        // debugging of the ninja files. However, they do not have to be
        // guaranteed to be unique, since NinjaWriter adds a sequence numbers
        // when adding to guarantee uniqueness.
        if let Some((_, args)) = rule_vars
            .iter()
            .find(|(name, _)| name.as_str() == "command")
        {
            let out = args
                .iter()
                .take(5)
                .map(|part| {
                    if let NinjaArg::Const(x) = part {
                        x.replace(|c: char| !c.is_alphabetic(), "")
                    } else {
                        "".to_string()
                    }
                })
                .filter(|el| !el.is_empty())
                .collect::<Vec<String>>()
                .join("_");
            if out.is_empty() {
                "rule".to_string()
            } else {
                out
            }
        } else {
            "rule".to_string()
        }
    }

    fn populate_ninja_file(&self, nf: &mut NinjaFile) -> NinjaRuleRef {
        if let Some(ruleref) = nf.get_rule_ref(self.id) {
            ruleref
        } else {
            /* Create rule base */
            // TODO: More than just index numbers of ninja rules
            let rule = nf.rule(self.id, &self.name);

            for (var_name, var_args) in self.rule_vars.iter() {
                rule.var(var_name, var_args.clone());
            }
            /* Sore reference and write back */
            rule.as_ref()
        }
    }
}

#[derive(PartialEq, Debug)]
pub struct PbBuild {
    id: usize,
    rule: Rc<PbBuildRule>,
    input: Vec<NinjaArg>,
    output: Vec<NinjaArg>,
    args: BTreeMap<String, Vec<NinjaArg>>,
    deps: Vec<Rc<PbBuild>>,
}

impl Display for PbBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(", self.rule.name)?;
        for o in self.output.iter() {
            if let NinjaArg::Path(op) = o {
                write!(f, " {}", op.to_path_buf().display())?;
            } else {
                write!(f, "??")?;
            }
        }
        write!(f, " <-",)?;
        for i in self.input.iter() {
            if let NinjaArg::Path(ip) = i {
                write!(f, " {}", ip.to_path_buf().display())?;
            } else {
                write!(f, "??")?;
            }
        }
        write!(f, " )")?;
        Ok(())
    }
}

impl PbBuild {
    pub fn populate_ninja_file(&self, nf: &mut NinjaFile) {
        if !nf.has_build(self.id) {
            for dep in self.deps.iter() {
                /* TODO: Block duplicates */
                dep.populate_ninja_file(nf);
            }

            let rule = self.rule.populate_ninja_file(nf);
            let build = nf.build(self.id, &rule);
            for inp in self.input.iter() {
                build.input(inp.clone());
            }
            for outp in self.output.iter() {
                build.output(outp.clone());
            }
            for (var_name, var_attrs) in self.args.iter() {
                build.var(var_name, var_attrs.clone());
            }
        }
    }
}

fn value_to_ninja_arg(attr: &Value) -> NinjaArg {
    match attr {
        Value::Int(value) => NinjaArg::Const(format!("{}", value)),
        Value::String(value) => NinjaArg::Const(value.clone()),
        Value::Path(path) => NinjaArg::Path(path.clone()),
        Value::Build(build) => {
            assert_eq!(build.output.len(), 1); // TODO: generic handling of builds
            build.output[0].clone()
        }
        Value::BuildVar(value) => NinjaArg::Var(value.clone()),
        Value::BuildConcat(vs) => NinjaArg::Concat(
            vs.iter()
                .map(|v| match v {
                    Value::Int(value) => NinjaArg::Const(format!("{}", value)),
                    Value::String(value) => NinjaArg::Const(value.clone()),
                    Value::BuildVar(value) => NinjaArg::Var(value.clone()),
                    Value::Path(path) => NinjaArg::Path(path.clone()),
                    _ => unreachable!(),
                })
                .collect(),
        ),
        _ => panic!("Rule attr is of invalid type: {}", attr),
    }
}

#[derive(Debug)]
pub struct BuiltinPbRule;

impl<F> ExprBuiltin<Value, F> for BuiltinPbRule
where
    F: Clone + Debug,
{
    fn get_name(&self) -> String {
        "build".into()
    }

    fn call(&self, arg: crate::lang::Expr<Value, F>) -> Result<Expr<Value, F>, F> {
        arg.resolve()?;
        let loc = arg.get_loc();

        /* Initialize meta variables, that may change later */
        let mut rule_args: BTreeSet<String> = BTreeSet::new();

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
                    rule_args.insert(name.clone());

                    /* Generate element */
                    Ok((
                        name.clone(),
                        ExprType::from(Value::BuildVar(match name.as_str() {
                            "input" => "in".into(),
                            "output" => "out".into(),
                            _ => name.clone(),
                        }))
                        .reref(loc.clone()),
                    ))
                })
                .collect::<Result<ExprSet<Value, F>, F>>()?,
        )
        .reref(loc.clone());

        /* Generate rule function with variable placeholders and call */
        let rule_func: Expr<Value, F> = ExprType::FuncCall(var_obj, arg).reref(loc.clone());
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
        let mut vars: Vec<(String, Vec<NinjaArg>)> = Vec::new();
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
        Ok(ExprType::from(Value::BuildRule(PbBuildRule::new(rule_args, vars).into())).reref(loc))
    }
}

#[derive(Debug)]
pub struct BuiltinPbBuild;

impl<F> ExprBuiltin<Value, F> for BuiltinPbBuild
where
    F: Clone + Debug,
{
    fn get_name(&self) -> String {
        "build".into()
    }

    fn call(&self, arg: crate::lang::Expr<Value, F>) -> Result<crate::lang::Expr<Value, F>, F> {
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
        let rule = expr_get_arg!(arg_obj, "rule", try_as_build_rule);

        /* Read all variables required by rule */
        let mut args: BTreeMap<String, Vec<NinjaArg>> = BTreeMap::new();
        /* Special treatment for input/output */
        let mut input: Vec<NinjaArg> = vec![];
        let mut output: Vec<NinjaArg> = vec![];
        /* Track all dependent rules, that needs to be added to ninja file  */
        let mut deps: Vec<Rc<PbBuild>> = vec![];

        for arg_name in rule.rule_args.iter() {
            /* Read variable */
            let build_arg = expr_get_arg!(arg_obj, arg_name);
            build_arg.resolve()?;

            let mut value: Vec<NinjaArg> = vec![];

            let elems: Vec<Expr<Value, F>> = match &build_arg.inner_ref().tok {
                ExprType::List(exprs) => Ok(exprs.clone()),
                ExprType::Value(value) => {
                    Ok(vec![ExprType::from(value.clone()).reref(loc.clone())])
                }
                _ => Err(Error::new(
                    ErrorType::Type,
                    format!("field {} is not a list or value", arg_name),
                )),
            }?;

            for elem in elems.into_iter() {
                elem.resolve()?;
                value.push(match &elem.inner_ref().tok {
                    ExprType::Value(attr) => {
                        if let Value::Build(build) = attr {
                            deps.push(build.clone());
                        }
                        Ok(value_to_ninja_arg(attr))
                    }
                    _ => Err(Error::new(
                        ErrorType::Type,
                        format!("incompatible type in build arg {}", arg_name),
                    )),
                }?);
            }

            match arg_name.as_str() {
                "input" => input = value,
                "output" => output = value,
                name => {
                    args.insert(name.to_string(), value);
                }
            }
        }

        Ok(ExprType::Value(Value::Build(Rc::new(PbBuild {
            id: unique_id(),
            rule,
            input,
            output,
            args,
            deps,
        })))
        .reref(loc))
    }
}

#[derive(Debug)]
pub struct BuiltinPbLock;

impl ExprBuiltin<Value, VirtPath> for BuiltinPbLock {
    fn get_name(&self) -> String {
        "lock".into()
    }

    fn call(&self, arg: Expr<Value, VirtPath>) -> Result<Expr<Value, VirtPath>, VirtPath> {
        let val = arg.value()?;
        let path = val.try_as_path().ok_or(
            Error::new(ErrorType::Type, format!("expected path, got {}", arg))
                .reref(&arg.get_loc()),
        )?;
        Ok(ExprType::Value(Value::Path(path.lock())).reref(arg.get_loc()))
    }
}

#[derive(Debug)]
pub struct BuiltinPbTranslate;

impl ExprBuiltin<Value, VirtPath> for BuiltinPbTranslate {
    fn get_name(&self) -> String {
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
            .value()?
            .try_as_path()
            .ok_or_else(|| Error::new(ErrorType::Type, "expected path").reref(&input.get_loc()))?;
        let from = from
            .value()?
            .try_as_path()
            .ok_or_else(|| Error::new(ErrorType::Type, "expected path").reref(&from.get_loc()))?;
        let to = to
            .value()?
            .try_as_path()
            .ok_or_else(|| Error::new(ErrorType::Type, "expected path").reref(&to.get_loc()))?;

        // Clone here only to allow error message
        let output = input.clone().translate(&from, &to).ok_or_else(|| {
            Error::new(
                ErrorType::Type,
                format!("Can't translate {} from {} to {}", input, from, to),
            )
            .reref(&loc)
        })?;

        Ok(ExprType::Value(Value::Path(output)).reref(loc))
    }
}

pub fn get_pb_builtins() -> Result<Expr<Value, VirtPath>, VirtPath> {
    let pbset = ExprSet::from([
        ("lock".into(), Expr::new_builtin(Rc::new(BuiltinPbLock))),
        ("rule".into(), Expr::new_builtin(Rc::new(BuiltinPbRule))),
        ("build".into(), Expr::new_builtin(Rc::new(BuiltinPbBuild))),
        (
            "translate".into(),
            Expr::new_builtin(Rc::new(BuiltinPbTranslate)),
        ),
    ]);
    Ok(ExprType::Object(pbset).builtin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_id() {
        /* Just guard against obvious errors with static var here... */
        let mut set: BTreeSet<usize> = BTreeSet::new();
        for _ in 0..1000 {
            let id = unique_id();
            assert!(set.insert(id));
        }
    }
}
