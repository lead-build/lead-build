use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    rc::Rc,
};

use crate::{
    Expr,
    lang::{ExprSet, ExprType, Result, ops::ExprBuiltin},
    ninjawriter::{NinjaArg, NinjaFile, NinjaRuleRef},
    value::Value,
};

macro_rules! expr_get_arg (
    ($obj:expr, $name:expr, $unpack:ident) => {
        $obj
            .remove_mut($name)
            .ok_or_else(|| crate::lang::ops::Error::Type(format!("Can't unpack {}", stringify!($name))))?
            .value()?
            .$unpack()
            .ok_or_else(|| crate::lang::ops::Error::Type(format!("Can't unpack {}", stringify!($name))))?
    };
    ($obj:expr, $name:expr) => {
        $obj
            .remove_mut($name)
            .ok_or_else(|| crate::lang::ops::Error::Type(format!("Can't unpack {}", stringify!($name))))?
    };
);

#[derive(PartialEq, Debug)]
pub struct PbBuildRule {
    reference: RefCell<Option<NinjaRuleRef>>,
    name: String,
    rule_args: BTreeSet<String>,
    rule_vars: Vec<(String, Vec<NinjaArg>)>,
}

impl Display for PbBuildRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BuildRule()")
    }
}

impl PbBuildRule {
    fn new(rule_args: BTreeSet<String>, rule_vars: Vec<(String, Vec<NinjaArg>)>) -> Self {
        PbBuildRule {
            reference: None.into(),
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
        /*
         * If already generated, just output reference
         */
        if let Some(rule) = &*self.reference.borrow() {
            return rule.clone();
        }

        /* Create rule base */
        // TODO: More than just index numbers of ninja rules
        let rule = nf.rule(&self.name);

        for (var_name, var_args) in self.rule_vars.iter() {
            rule.var(var_name, var_args.clone());
        }
        /* Sore reference and write back */
        let ruleref = rule.as_ref();
        self.reference.replace(Some(ruleref.clone()));
        ruleref
    }
}

#[derive(PartialEq, Debug)]
pub struct PbBuild {
    rule: Rc<PbBuildRule>,
    input: Vec<NinjaArg>,
    output: Vec<NinjaArg>,
    args: BTreeMap<String, Vec<NinjaArg>>,
    deps: Vec<Rc<PbBuild>>,
}

impl Display for PbBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Build({})", self.rule)
    }
}

impl PbBuild {
    pub fn populate_ninja_file(&self, nf: &mut NinjaFile) {
        for dep in self.deps.iter() {
            /* TODO: Block duplicates */
            dep.populate_ninja_file(nf);
        }


        let rule = self.rule.populate_ninja_file(nf);
        let build = nf.build(&rule);
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
                    _ => unreachable!(),
                })
                .collect(),
        ),
        _ => panic!("Rule attr is of invalid type: {}", attr),
    }
}

#[derive(Debug)]
pub struct BuiltinPbRule;

impl ExprBuiltin<Value> for BuiltinPbRule {
    fn get_name(&self) -> String {
        "build".into()
    }

    fn call(
        &self,
        arg: crate::lang::Expr<Value>,
    ) -> crate::lang::ops::Result<crate::lang::Expr<Value>> {
        arg.resolve()?;

        /* Initialize meta variables, that may change later */
        let mut rule_args: BTreeSet<String> = BTreeSet::new();

        /* Identify arguments */
        let args = match arg.as_ref().try_as_func_def_pattern_ref() {
            Some((items, _expr)) => Ok(items.clone()),
            None => Err(crate::lang::ops::Error::Type(
                "pb.rule needs to take a pattern function as argument".into(),
            )),
        }?;

        /* Generate object with placeholders */
        let var_obj = ExprSet::from(args.iter().map(|name| {
            /* Also store names for validation from PbBuild */
            rule_args.insert(name.clone());

            /* Generate element */
            (
                name.clone(),
                Value::BuildVar(match name.as_str() {
                    "input" => "in".into(),
                    "output" => "out".into(),
                    _ => name.clone(),
                })
                .into(),
            )
        }))?
        .into();

        /* Generate rule function with variable placeholders and call */
        let rule_func: Expr<Value> = ExprType::FuncCall(arg, var_obj).into();
        rule_func.resolve()?;

        /* Read variables */
        let objargs = match rule_func.as_ref().try_as_object_ref() {
            Some(args) => Ok(args.clone()),
            None => Err(crate::lang::ops::Error::Type(format!(
                "pb.rule function needs to return an object, got {}",
                rule_func
            ))),
        }?;

        /* Convert all variables to ninja rule */
        let mut vars: Vec<(String, Vec<NinjaArg>)> = Vec::new();
        for (name, expr) in objargs.into_vec().into_iter() {
            expr.resolve()?;
            let attrs = match &*expr.as_ref() {
                ExprType::List(exprs) => exprs.clone(),
                ExprType::Value(value) => vec![value.clone().into()],
                _ => panic!("pb.rule function needs to return an object"),
            };
            let ninja_attrs: Vec<NinjaArg> = attrs
                .into_iter()
                .map(|e| {
                    e.resolve().unwrap();
                    match &*e.as_ref() {
                        ExprType::Value(attr) => value_to_ninja_arg(attr),
                        _ => panic!("Rule attr is not a value"),
                    }
                })
                .collect();

            vars.push((name, ninja_attrs));
        }

        /* Wrap into a node */
        Ok(Value::BuildRule(PbBuildRule::new(rule_args, vars).into()).into())
    }
}

#[derive(Debug)]
pub struct BuiltinPbBuild;

impl ExprBuiltin<Value> for BuiltinPbBuild {
    fn get_name(&self) -> String {
        "build".into()
    }

    fn call(
        &self,
        arg: crate::lang::Expr<Value>,
    ) -> crate::lang::ops::Result<crate::lang::Expr<Value>> {
        arg.resolve()?;

        let opt_err =
            || crate::lang::ops::Error::Type(format!("unknown arg for pb.build, got {}", arg));

        /* Read arguments from input object */
        let mut arg_obj = arg.as_ref().clone().try_as_object().ok_or_else(opt_err)?;
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

            let elems: Vec<Expr<Value>> = match &*build_arg.as_ref() {
                ExprType::List(exprs) => Ok(exprs.clone()),
                ExprType::Value(value) => Ok(vec![value.clone().into()]),
                _ => Err(crate::lang::ops::Error::Type(format!(
                    "field {} is not a list or value",
                    arg_name
                ))),
            }?;

            for elem in elems.into_iter() {
                elem.resolve()?;
                value.push(match &*elem.as_ref() {
                    ExprType::Value(attr) => {
                        if let Value::Build(build) = attr {
                            deps.push(build.clone());
                        }
                        Ok(value_to_ninja_arg(attr))
                    }
                    _ => Err(crate::lang::ops::Error::Type(format!(
                        "incompatible type in build arg {}",
                        arg_name
                    ))),
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
            rule,
            input,
            output,
            args,
            deps,
        })))
        .into())
    }
}

pub fn get_pb_builtins() -> Result<Expr<Value>> {
    let pbset = ExprSet::new()
        .set("rule", Expr::new_builtin(Rc::new(BuiltinPbRule)))?
        .set("build", Expr::new_builtin(Rc::new(BuiltinPbBuild)))?;
    Ok(pbset.into())
}
