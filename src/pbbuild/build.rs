use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::{Debug, Display},
    rc::Rc,
};

use crate::{
    pbexpr::{Error, ErrorType, Referrable, Result},
    strkey::StrKey,
};

use super::{
    ninjawriter::{NinjaArg, NinjaBuild, NinjaBuildRef, NinjaFile, NinjaRule, NinjaRuleRef},
    path::VirtPath,
};

#[derive(PartialEq, Debug)]
pub struct PbBuildRule {
    rule_args: BTreeSet<StrKey>,
    rule_vars: Vec<(StrKey, Vec<NinjaArg>)>,
}

impl Display for PbBuildRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BuildRule")
    }
}

impl PbBuildRule {
    pub(crate) fn new(
        rule_args: BTreeSet<StrKey>,
        rule_vars: Vec<(StrKey, Vec<NinjaArg>)>,
    ) -> Self {
        PbBuildRule {
            rule_args,
            rule_vars,
        }
    }

    pub(crate) fn rule_args(&self) -> &BTreeSet<StrKey> {
        &self.rule_args
    }

    fn populate_ninja_file(&self, nf: &mut NinjaFile) -> NinjaRuleRef {
        let mut rule = NinjaRule::new();

        for (var_name, var_args) in self.rule_vars.iter() {
            rule.var(var_name, var_args.clone());
        }

        nf.add_rule(rule)
    }
}

/// The builds already added to a ninja file, by address, each with its
/// reference in the file. A build that many outputs depend on, such as the
/// object file of a module that many programs link, is reached once for each
/// of them, and adding it again would only clone and hash it to find it is
/// there already. Each build is kept alive here, so that its address cannot
/// be taken by another build while the file is generated.
#[derive(Default)]
pub struct PopulatedBuilds(HashMap<*const PbBuild, (Rc<PbBuild>, NinjaBuildRef)>);

#[derive(PartialEq, Debug)]
pub struct PbBuild {
    rule: Rc<PbBuildRule>,
    input: Vec<VirtPath>,
    output: Vec<VirtPath>,
    deps: Vec<VirtPath>,
    args: BTreeMap<StrKey, Vec<NinjaArg>>,
    dep_builds: Vec<Rc<PbBuild>>,
}

impl Display for PbBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(", self.rule)?;
        for o in self.output.iter() {
            write!(f, " {}", o.to_path_buf().display())?;
        }
        write!(f, " for")?;
        for i in self.input.iter() {
            write!(f, " {}", i.to_path_buf().display())?;
        }
        write!(f, " )")?;
        Ok(())
    }
}

impl PbBuild {
    pub(crate) fn new(
        rule: Rc<PbBuildRule>,
        input: Vec<VirtPath>,
        output: Vec<VirtPath>,
        deps: Vec<VirtPath>,
        args: BTreeMap<StrKey, Vec<NinjaArg>>,
        dep_builds: Vec<Rc<PbBuild>>,
    ) -> Self {
        PbBuild {
            rule,
            input,
            output,
            deps,
            args,
            dep_builds,
        }
    }

    pub fn ninja_outputs(&self) -> &Vec<VirtPath> {
        &self.output
    }

    /// Add the build and the builds it depends on to `nf`, each once.
    pub fn populate_ninja_file<F: Clone + Debug + Referrable>(
        self: &Rc<Self>,
        nf: &mut NinjaFile,
        populated: &mut PopulatedBuilds,
        is_default: bool,
    ) -> Result<(), F> {
        if let Some((_, build_ref)) = populated.0.get(&Rc::as_ptr(self)) {
            // Added already, with the builds it depends on
            if is_default {
                let build_ref = build_ref.clone();
                nf.set_build_default(&build_ref);
            }
            return Ok(());
        }

        for dep in self.dep_builds.iter() {
            dep.populate_ninja_file(nf, populated, false)?;
        }

        let rule = self.rule.populate_ninja_file(nf);
        let mut build = NinjaBuild::new(&rule);
        for inp in self.input.iter() {
            build.input(inp.clone());
        }
        for outp in self.output.iter() {
            build.output(outp.clone());
        }
        for dep in self.deps.iter() {
            build.dep(dep.clone());
        }
        for (var_name, var_attrs) in self.args.iter() {
            build.var(var_name, var_attrs.clone());
        }

        let build_ref = nf
            .add_build(build)
            .map_err(|message| Error::new(ErrorType::Custom, message))?;

        if is_default {
            nf.set_build_default(&build_ref);
        }
        populated
            .0
            .insert(Rc::as_ptr(self), (Rc::clone(self), build_ref));

        Ok(())
    }

    pub fn get_output<F: Clone>(&self) -> Result<VirtPath, F> {
        if self.output.len() == 1 {
            return Ok(self.output[0].clone());
        }
        Err(Error::new(
            ErrorType::Custom,
            "Expected exactly one output path",
        ))
    }
}
