use std::{cell::RefCell, fmt::Display, rc::Rc};

use crate::{
    Expr,
    lang::{ExprSet, ExprType, Result, ops::ExprBuiltin},
    ninjawriter::{NinjaArg, NinjaFile, NinjaRuleRef},
    path::VirtPath,
    value::Value,
};

#[derive(PartialEq, Debug)]
pub struct PbBuildRule {
    reference: RefCell<Option<NinjaRuleRef>>,
    expr: Expr<Value>,
}

impl Display for PbBuildRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BuildRule()")
    }
}

impl From<Expr<Value>> for PbBuildRule {
    fn from(value: Expr<Value>) -> Self {
        PbBuildRule {
            reference: None.into(),
            expr: value,
        }
    }
}

impl PbBuildRule {
    fn populate_ninja_file(&self, nf: &mut NinjaFile) -> NinjaRuleRef {
        /*
         * If already generated, just output reference
         */
        if let Some(rule) = &*self.reference.borrow() {
            return rule.clone();
        }

        /* Create rule base */
        let mut rule = nf.rule("name");

        /* Read variables */
        /* TODO: Error handling instead of unwrap */
        self.expr.resolve().unwrap();

        let objargs = match self.expr.as_ref().try_as_object_ref() {
            Some(args) => args.clone(),
            None => panic!(
                "pb.rule function needs to return an object, got {}",
                self.expr
            ),
        };

        for (name, expr) in objargs.into_vec().into_iter() {
            expr.resolve().unwrap();
            let attrs = match &*expr.as_ref() {
                ExprType::List(exprs) => exprs.clone(),
                ExprType::Value(value) => vec![value.clone().into()],
                _ => panic!("pb.rule function needs to return an object"),
            };
            let ninja_attrs: Vec<_> = attrs
                .into_iter()
                .map(|e| {
                    e.resolve().unwrap();
                    match &*e.as_ref() {
                        ExprType::Value(attr) => match attr {
                            Value::Int(value) => NinjaArg::Const(format!("{}", value)),
                            Value::String(value) => NinjaArg::Const(value.clone()),
                            Value::BuildVar(value) => NinjaArg::Var(value.clone()),
                            _ => panic!("Rule attr is of invalid type: {}", attr),
                        },
                        _ => panic!("Rule attr is not a value"),
                    }
                })
                .collect();
            rule = rule.var(name, ninja_attrs);
        }

        /* Sore reference and write back */
        let ruleref = rule.as_ref();
        self.reference.replace(Some(ruleref.clone()));
        ruleref
    }
}

#[derive(PartialEq, Debug)]
pub struct PbBuild {
    output: VirtPath,
    input: Vec<VirtPath>,
    rule: Rc<PbBuildRule>,
}

impl Display for PbBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Build({})", self.output)
    }
}

impl PbBuild {
    pub fn populate_ninja_file(&self, nf: &mut NinjaFile) {
        let rule = self.rule.populate_ninja_file(nf);
        nf.build(&rule).input("in").output("out");
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

        /* Identify arguments */
        let items = match arg.as_ref().try_as_func_def_pattern_ref() {
            Some((items, _expr)) => Ok(items.clone()),
            None => Err(crate::lang::ops::Error::Type(
                "pb.rule needs to take a pattern function as argument".into(),
            )),
        }?;

        /* Generate object with placeholders */
        let var_obj = ExprSet::from(items.into_iter().map(|name| {
            (
                name.clone(),
                Value::BuildVar(match name.as_str() {
                    "input" => "in".into(),
                    "output" => "out".into(),
                    _ => name,
                })
                .into(),
            )
        }))?
        .into();

        /* Generate rule function with variable placeholders */
        let rule_func: Expr<Value> = ExprType::FuncCall(arg, var_obj).into();

        /* Wrap into a node */
        Ok(Value::BuildRule(PbBuildRule::from(rule_func).into()).into())
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
        let output = arg.get_item("output")?.value()?;
        let rule = arg.get_item("rule")?.value()?;
        let output = output
            .try_as_path()
            .ok_or(crate::lang::ops::Error::Type(format!(
                "expected path, got {}",
                arg
            )))?;
        let rule = rule
            .try_as_build_rule()
            .ok_or(crate::lang::ops::Error::Type(format!(
                "expected build rule, got {}",
                arg
            )))?;
        let input = vec![];
        Ok(ExprType::Value(Value::Build(Rc::new(PbBuild {
            output,
            input,
            rule,
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
