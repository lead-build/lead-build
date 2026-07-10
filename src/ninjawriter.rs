use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt::Display,
};

use crate::path::VirtPath;

/*
 * Model
 */

#[derive(Debug, PartialEq, Clone)]
pub enum NinjaArg {
    Const(String),
    Var(String),
    Path(VirtPath),
    Concat(Vec<NinjaArg>),
}

#[derive(Default, Debug)]
struct UniqueNames {
    names: BTreeSet<String>,
}

#[derive(Debug)]
pub struct NinjaVar {
    name: String,
    args: Vec<NinjaArg>,
}

#[derive(Debug, Default)]
pub struct NinjaRule {
    name: String,
    vars: Vec<NinjaVar>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct NinjaRuleRef(String);

#[derive(Debug, Default)]
pub struct NinjaBuild {
    rule: String,
    outputs: Vec<NinjaArg>,
    inputs: Vec<NinjaArg>,
    deps: Vec<NinjaArg>,
    vars: Vec<NinjaVar>,
    is_default: bool,
}

#[derive(Debug, Default)]
pub struct NinjaFile {
    rule_names: UniqueNames,
    rules: BTreeMap<usize, NinjaRule>,
    builds: BTreeMap<usize, NinjaBuild>,
    aliases: Vec<NinjaBuild>,
}

/*
 * From
 */

impl From<&str> for NinjaArg {
    fn from(value: &str) -> Self {
        NinjaArg::Const(value.into())
    }
}

/*
 * Display
 */

fn ninja_indent(f: &mut std::fmt::Formatter<'_>, indent: i32) -> std::fmt::Result {
    for _ in 0..indent {
        write!(f, "  ")?;
    }
    Ok(())
}

fn ninja_esc_string(f: &mut std::fmt::Formatter<'_>, indent: i32, input: &str) -> std::fmt::Result {
    for c in input.chars() {
        match c {
            '$' => write!(f, "$$")?,
            '\n' => {
                writeln!(f, "$")?;
                ninja_indent(f, indent)?;
            }
            ':' => write!(f, "$:")?,
            ' ' => write!(f, "$ ")?,
            c => write!(f, "{}", c)?,
        }
    }
    Ok(())
}

impl NinjaArg {
    fn write(&self, indent: i32, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NinjaArg::Const(cnst) => ninja_esc_string(f, indent + 1, cnst),
            NinjaArg::Var(name) => write!(f, "${{{}}}", name),
            NinjaArg::Path(path) => write!(f, "{}", path.clone().to_path_buf().display()), // TODO: Handle paths
            NinjaArg::Concat(ninja_args) => {
                for subarg in ninja_args.iter() {
                    subarg.write(indent, f)?;
                }
                Ok(())
            }
        }
    }
}

impl NinjaVar {
    fn write(&self, indent: i32, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ninja_indent(f, indent)?;
        ninja_esc_string(f, indent + 1, &self.name)?;
        write!(f, " =")?;
        for arg in self.args.iter() {
            write!(f, " ")?;
            arg.write(indent, f)?;
        }
        writeln!(f)?;
        Ok(())
    }
}

impl Display for NinjaRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rule ")?;
        ninja_esc_string(f, 1, &self.name)?;
        writeln!(f)?;
        for var in self.vars.iter() {
            var.write(1, f)?;
        }
        writeln!(f)?;
        Ok(())
    }
}

impl Display for NinjaBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "build")?;
        for outp in self.outputs.iter() {
            write!(f, " ")?;
            outp.write(1, f)?;
        }
        write!(f, ": ")?;
        ninja_esc_string(f, 1, &self.rule)?;
        for inp in self.inputs.iter() {
            write!(f, " ")?;
            inp.write(1, f)?;
        }
        if !self.deps.is_empty() {
            write!(f, " |")?;
            for dep in self.deps.iter() {
                write!(f, " ")?;
                dep.write(1, f)?;
            }
        }
        writeln!(f)?;
        for var in self.vars.iter() {
            var.write(1, f)?;
        }
        writeln!(f)?;
        if self.is_default {
            write!(f, "default")?;
            for outp in self.outputs.iter() {
                write!(f, " ")?;
                outp.write(1, f)?;
            }
            writeln!(f)?;
            writeln!(f)?;
        }
        Ok(())
    }
}

impl Display for NinjaFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (_, rule) in self.rules.iter() {
            rule.fmt(f)?;
        }
        for (_, build) in self.builds.iter() {
            build.fmt(f)?;
        }
        for alias in self.aliases.iter() {
            alias.fmt(f)?;
        }
        Ok(())
    }
}

/*
 * Tools
 */
impl UniqueNames {
    fn get(&mut self, name: impl ToString) -> String {
        let name = name.to_string();
        if self.names.insert(name.clone()) {
            return name;
        }

        for idx in 1.. {
            let indexed_name = format!("{}{}", name, idx);
            if self.names.insert(indexed_name.clone()) {
                return indexed_name;
            }
        }
        unreachable!()
    }
}

/*
 * Construction
 */

impl NinjaRule {
    fn new(name: impl ToString) -> Self {
        NinjaRule {
            name: name.to_string(),
            ..Default::default()
        }
    }

    pub fn var(&mut self, name: impl ToString, args: Vec<NinjaArg>) -> &mut Self {
        self.vars.push(NinjaVar {
            name: name.to_string(),
            args,
        });
        self
    }

    pub fn as_ref(&self) -> NinjaRuleRef {
        NinjaRuleRef(self.name.clone())
    }
}

impl NinjaBuild {
    fn new(rule: &NinjaRuleRef) -> Self {
        NinjaBuild {
            rule: rule.0.clone(),
            is_default: false,
            ..Default::default()
        }
    }

    fn alias() -> Self {
        NinjaBuild::new(&NinjaRuleRef("phony".into()))
    }

    pub fn output(&mut self, name: NinjaArg) -> &mut Self {
        self.outputs.push(name);
        self
    }

    pub fn input(&mut self, name: NinjaArg) -> &mut Self {
        self.inputs.push(name);
        self
    }

    pub fn dep(&mut self, name: NinjaArg) -> &mut Self {
        self.deps.push(name);
        self
    }

    pub fn var(&mut self, name: impl ToString, args: Vec<NinjaArg>) -> &mut Self {
        self.vars.push(NinjaVar {
            name: name.to_string(),
            args,
        });
        self
    }

    pub fn set_default(&mut self) -> &mut Self {
        self.is_default = true;
        self
    }
}

impl NinjaFile {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn rule(&mut self, id: usize, name: impl ToString) -> &mut NinjaRule {
        let unique_name = self.rule_names.get(name);
        self.rules.insert(id, NinjaRule::new(unique_name));
        self.rules.get_mut(&id).unwrap()
    }

    pub fn build(&mut self, id: usize, rule: &NinjaRuleRef) -> &mut NinjaBuild {
        self.builds.insert(id, NinjaBuild::new(rule));
        self.builds.get_mut(&id).unwrap()
    }

    pub fn alias(&mut self, name: impl ToString) -> &mut NinjaBuild {
        let mut alias = NinjaBuild::alias();
        alias.output(NinjaArg::Const(name.to_string()));

        self.aliases.push(alias);
        self.aliases.last_mut().unwrap()
    }

    pub fn get_rule_ref(&mut self, id: usize) -> Option<NinjaRuleRef> {
        self.rules.get(&id).map(|rule| rule.as_ref())
    }

    pub fn validate(&self) -> Vec<String> {
        // TODO: Better interface than returing string of messages

        let mut errors: BTreeSet<String> = BTreeSet::new();
        let mut output_set = HashSet::new();
        for build in self
            .builds
            .values()
            .chain(self.aliases.iter())
        {
            for output in build.outputs.iter() {
                match output {
                    NinjaArg::Path(file) => {
                        let fs_path = file.to_path_buf();
                        if !output_set.insert(fs_path.clone()) {
                            errors.insert(format!("Multiple builds generating: {}", fs_path.display()));
                        }
                    }
                    NinjaArg::Const(_) if build.rule == "phony" => {
                        // Alias targets are expected to use symbolic names.
                    }
                    _ => {
                        errors.insert(format!("Non-path build output: {:?}", output));
                    }
                }
            }
        }
        errors.into_iter().collect()
    }

    pub fn get_build(&mut self, id: usize) -> Option<&mut NinjaBuild> {
        self.builds.get_mut(&id)
    }
}

/*
 * Tests
 */

#[cfg(test)]
mod tests {
    use super::*;

    type TestF = i32;

    macro_rules! lines (
        ($line:expr) => ($line);
        ($line:expr, $($rest:expr),+) => (concat!($line, "\n", lines!($($rest),+)));
        () => ("");
    );

    #[test]
    fn test_write_rule() {
        let mut rule = NinjaRule::new("test");
        rule.var("deps", vec!["boll".into(), "something".into()])
            .var(
                "something",
                vec!["stuff".into(), "stuff".into(), NinjaArg::Var("in".into())],
            );
        assert_eq!(
            format!("{}", rule).as_str(),
            lines! {
                "rule test",
                "  deps = boll something",
                "  something = stuff stuff ${in}",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_esc_string() {
        let mut rule = NinjaRule::new("r$a");
        rule.var("a b", vec!["a$b".into(), "a b".into()])
            .var("b", vec!["a\nb".into(), "a:b".into()]);
        assert_eq!(
            format!("{}", rule).as_str(),
            lines! {
                "rule r$$a",
                "  a$ b = a$$b a$ b",
                "  b = a$",
                "    b a$:b",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_build() {
        let mut rule = NinjaRule::new("r$a");
        rule.var("a b", vec!["a$b".into(), "a b".into()])
            .var("b", vec!["a\nb".into(), "a:b".into()]);
        let mut build = NinjaBuild::new(&rule.as_ref());
        build
            .input(NinjaArg::Const("boll".into()))
            .input(NinjaArg::Const("hej".into()))
            .output(NinjaArg::Const("dest".into()))
            .output(NinjaArg::Const("destb".into()))
            .var("tjo", vec!["xx".into()]);
        let output = format!("{}{}", rule, build);
        assert_eq!(
            output.as_str(),
            lines! {
                "rule r$$a",
                "  a$ b = a$$b a$ b",
                "  b = a$",
                "    b a$:b",
                "",
                "build dest destb: r$$a boll hej",
                "  tjo = xx",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_build_deps() {
        let rule = NinjaRule::new("rule");
        let mut build = NinjaBuild::new(&rule.as_ref());
        build
            .input(NinjaArg::Const("in".into()))
            .output(NinjaArg::Const("out".into()))
            .dep(NinjaArg::Const("dep".into()));
        let output = format!("{}{}", rule, build);
        assert_eq!(
            output.as_str(),
            lines! {
                "rule rule",
                "",
                "build out: rule in | dep",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_build_default() {
        let rule = NinjaRule::new("rule");
        let mut build = NinjaBuild::new(&rule.as_ref());
        build
            .input(NinjaArg::Const("in".into()))
            .output(NinjaArg::Const("out".into()))
            .set_default();
        let output = format!("{}{}", rule, build);
        assert_eq!(
            output.as_str(),
            lines! {
                "rule rule",
                "",
                "build out: rule in",
                "",
                "default out",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_file() {
        let mut file = NinjaFile::new();

        let rule1 = file
            .rule(1, "test1")
            .var("x", vec!["stuff".into()])
            .as_ref();
        let _rule2 = file
            .rule(2, "test2")
            .var("y", vec!["stuff".into()])
            .as_ref();

        file.build(3, &rule1)
            .input(NinjaArg::Const("in1_1".into()))
            .input(NinjaArg::Const("in1_2".into()))
            .output(NinjaArg::Const("out1".into()));

        assert_eq!(
            format!("{}", file),
            lines! {
                "rule test1",
                "  x = stuff",
                "",
                "rule test2",
                "  y = stuff",
                "",
                "build out1: test1 in1_1 in1_2",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_file_unique_rules() {
        let mut file = NinjaFile::new();

        assert_eq!(file.rule(1, "test").as_ref(), NinjaRuleRef("test".into()));
        assert_eq!(file.rule(2, "test").as_ref(), NinjaRuleRef("test1".into()));
        assert_eq!(file.rule(3, "x").as_ref(), NinjaRuleRef("x".into()));
        assert_eq!(file.rule(4, "test").as_ref(), NinjaRuleRef("test2".into()));
        assert_eq!(file.rule(5, "x").as_ref(), NinjaRuleRef("x1".into()));
        assert_eq!(file.rule(6, "test").as_ref(), NinjaRuleRef("test3".into()));
        assert_eq!(file.rule(7, "test").as_ref(), NinjaRuleRef("test4".into()));
        assert_eq!(file.rule(8, "x").as_ref(), NinjaRuleRef("x2".into()));
    }

    #[test]
    fn test_ref_unique_name() {
        let mut file = NinjaFile::new();

        let _rule = file.rule(1, "test").as_ref();
        let rule1 = file.rule(2, "test").as_ref();
        let rule2 = file.rule(3, "test").as_ref();

        file.build(4, &rule1)
            .output(NinjaArg::Path(
                VirtPath::new("root").step::<TestF>("out1").unwrap(),
            ))
            .set_default();
        file.build(5, &rule2).output(NinjaArg::Path(
            VirtPath::new("root").step::<TestF>("out2").unwrap(),
        ));

        assert_eq!(file.validate(), Vec::<String>::new());

        assert_eq!(
            format!("{}", file),
            lines! {
                "rule test",
                "",
                "rule test1",
                "",
                "rule test2",
                "",
                "build ./out1: test1",
                "",
                "default ./out1",
                "",
                "build ./out2: test2",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_variable_output_name() {
        let mut file = NinjaFile::new();
        let rule = file.rule(1, "test").as_ref();
        file.build(2, &rule).output(NinjaArg::Var("out1".into()));
        assert_eq!(file.validate().len(), 1);
    }

    #[test]
    fn test_multiple_same_targets() {
        let mut file = NinjaFile::new();
        let rule = file.rule(1, "test").as_ref();
        file.build(2, &rule).output(NinjaArg::Path(
            VirtPath::new("root").step::<TestF>("file").unwrap(),
        ));
        file.build(3, &rule).output(NinjaArg::Path(
            VirtPath::new("root").step::<TestF>("file").unwrap(),
        ));
        assert_eq!(
            file.validate(),
            vec!["Multiple builds generating: ./file".to_string()]
        );
    }
}
