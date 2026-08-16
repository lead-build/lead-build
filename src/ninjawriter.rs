use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Display,
};

use crate::path::VirtPath;
use log::debug;

/*
 * Model
 */

#[derive(Default, Debug)]
struct UniqueNames {
    names: BTreeSet<String>,
}

#[derive(Debug, PartialEq, Clone, Hash, Eq)]
pub enum NinjaArg {
    Const(String),
    Var(String),
    Path(VirtPath),
    Concat(Vec<NinjaArg>),
}

#[derive(Debug, PartialEq, Hash, Eq)]
pub struct NinjaVar {
    name: String,
    args: Vec<NinjaArg>,
}

#[derive(Debug, Default, PartialEq, Hash, Eq)]
pub struct NinjaRule {
    vars: Vec<NinjaVar>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct NinjaRuleRef(String);

#[derive(Debug, PartialEq, Clone, Hash, Eq)]
pub struct NinjaBuildRef(Vec<VirtPath>);

#[derive(Debug, Default, PartialEq, Hash, Eq)]
pub struct NinjaBuild {
    rule: String,
    outputs: Vec<VirtPath>,
    inputs: Vec<VirtPath>,
    deps: Vec<VirtPath>,
    vars: Vec<NinjaVar>,
}

#[derive(Debug, Default)]
pub struct NinjaFile {
    rule_names: UniqueNames,
    rules: HashMap<NinjaRule, String>,
    builds: HashSet<NinjaBuild>,
    build_outputs: HashSet<VirtPath>,
    aliases: HashMap<String, Vec<VirtPath>>,
    default_targets: HashSet<VirtPath>,
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

fn write_ninja_path(path: &VirtPath, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", path.clone().to_path_buf().display())
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

struct NamedNinjaRule<'a> {
    rule: &'a NinjaRule,
    name: &'a str,
}

impl Display for NamedNinjaRule<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rule ")?;
        ninja_esc_string(f, 1, self.name)?;
        writeln!(f)?;
        for var in self.rule.vars.iter() {
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
            write_ninja_path(outp, f)?;
        }
        write!(f, ": ")?;
        ninja_esc_string(f, 1, &self.rule)?;
        for inp in self.inputs.iter() {
            write!(f, " ")?;
            write_ninja_path(inp, f)?;
        }
        if !self.deps.is_empty() {
            write!(f, " |")?;
            for dep in self.deps.iter() {
                write!(f, " ")?;
                write_ninja_path(dep, f)?;
            }
        }
        writeln!(f)?;
        for var in self.vars.iter() {
            var.write(1, f)?;
        }
        writeln!(f)?;
        Ok(())
    }
}

impl Display for NinjaFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut rules = self.rules.iter().collect::<Vec<_>>();
        rules.sort_by(|left, right| left.1.cmp(right.1));

        for (rule, name) in rules {
            NamedNinjaRule { rule, name }.fmt(f)?;
        }

        let mut builds = self.builds.iter().collect::<Vec<_>>();
        builds.sort_by(|left, right| format!("{:?}", left).cmp(&format!("{:?}", right)));

        for build in builds {
            build.fmt(f)?;
        }

        if !self.aliases.is_empty() {
            let mut aliases = self.aliases.iter().collect::<Vec<_>>();
            aliases.sort_by(|left, right| left.0.cmp(right.0));

            for (name, inputs) in aliases {
                write!(f, "build ")?;
                ninja_esc_string(f, 1, name)?;
                write!(f, ": phony")?;
                for input in inputs.iter() {
                    write!(f, " ")?;
                    write_ninja_path(input, f)?;
                }
                writeln!(f)?;
                writeln!(f)?;
            }
        }

        if !self.default_targets.is_empty() {
            let mut defaults = self.default_targets.iter().collect::<Vec<_>>();
            defaults.sort_by(|left, right| format!("{:?}", left).cmp(&format!("{:?}", right)));

            write!(f, "default")?;
            for outp in defaults {
                write!(f, " ")?;
                write_ninja_path(outp, f)?;
            }
            writeln!(f)?;
            writeln!(f)?;
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
    pub fn new() -> Self {
        Default::default()
    }

    fn name_template(&self) -> String {
        if let Some(command) = self.vars.iter().find(|var| var.name == "command") {
            let command_hint = command
                .args
                .iter()
                .take(5)
                .filter_map(|part| match part {
                    NinjaArg::Const(text) => {
                        let cleaned = text.replace(|c: char| !c.is_alphabetic(), "");
                        if cleaned.is_empty() {
                            None
                        } else {
                            Some(cleaned)
                        }
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("_");

            if !command_hint.is_empty() {
                return command_hint;
            }
        }

        let var_hint = self
            .vars
            .iter()
            .map(|var| var.name.as_str())
            .take(3)
            .collect::<Vec<_>>()
            .join("_");

        if var_hint.is_empty() {
            "rule".to_string()
        } else {
            var_hint
        }
    }

    pub fn var(&mut self, name: impl ToString, args: Vec<NinjaArg>) -> &mut Self {
        self.vars.push(NinjaVar {
            name: name.to_string(),
            args,
        });
        self
    }
}

impl NinjaBuild {
    pub fn new(rule: &NinjaRuleRef) -> Self {
        NinjaBuild {
            rule: rule.0.clone(),
            ..Default::default()
        }
    }

    pub fn output(&mut self, name: VirtPath) -> &mut Self {
        self.outputs.push(name);
        self
    }

    pub fn input(&mut self, name: VirtPath) -> &mut Self {
        self.inputs.push(name);
        self
    }

    pub fn dep(&mut self, name: VirtPath) -> &mut Self {
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

    pub fn build_ref(&self) -> NinjaBuildRef {
        NinjaBuildRef(self.outputs.clone())
    }
}

impl NinjaFile {
    pub fn new() -> Self {
        Default::default()
    }

    fn find_rule_by_name(&self, name: &str) -> Option<(&NinjaRule, &str)> {
        self.rules
            .iter()
            .find(|(_, rule_name)| rule_name.as_str() == name)
            .map(|(rule, rule_name)| (rule, rule_name.as_str()))
    }

    fn find_build_rule(&self, build: &NinjaBuild) -> Option<(&NinjaRule, &str)> {
        self.find_rule_by_name(&build.rule)
    }

    pub fn add_rule(&mut self, rule: NinjaRule) -> NinjaRuleRef {
        let requested_name = rule.name_template();

        if let Some(existing) = self.rules.get(&rule) {
            debug!("DUP rule {}", existing);
            return NinjaRuleRef(existing.clone());
        }

        let rule_name = self.rule_names.get(requested_name);
        debug!("new rule {}", rule_name);
        let ruleref = NinjaRuleRef(rule_name.clone());
        self.rules.insert(rule, rule_name);
        ruleref
    }

    pub fn add_build(&mut self, build: NinjaBuild) -> Result<NinjaBuildRef, String> {
        debug!(
            "build {}: {}",
            build
                .outputs
                .iter()
                .map(|output| output.to_path_buf().display().to_string())
                .collect::<Vec<_>>()
                .join(" "),
            build.rule
        );

        if let Some(existing) = self.builds.get(&build) {
            return Ok(existing.build_ref());
        }

        let conflicting_outputs: Vec<VirtPath> = build
            .outputs
            .iter()
            .filter(|output| self.build_outputs.contains(*output))
            .cloned()
            .collect();

        if !conflicting_outputs.is_empty() {
            let conflicting_builds: Vec<&NinjaBuild> = self
                .builds
                .iter()
                .filter(|existing| {
                    existing
                        .outputs
                        .iter()
                        .any(|output| conflicting_outputs.contains(output))
                })
                .collect();

            let existing_conflicts: Vec<String> = conflicting_builds
                .iter()
                .map(|existing| format!("{}", existing))
                .collect();

            let existing_rules: Vec<String> = self
                .builds
                .iter()
                .filter(|existing| {
                    existing
                        .outputs
                        .iter()
                        .any(|output| conflicting_outputs.contains(output))
                })
                .map(|existing| existing.rule.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();

            let existing_rule_defs: Vec<String> = conflicting_builds
                .iter()
                .filter_map(|existing| self.find_build_rule(existing))
                .map(|(rule, name)| format!("{}", NamedNinjaRule { rule, name }))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();

            let new_rule_def = self
                .find_build_rule(&build)
                .map(|(rule, name)| format!("{}", NamedNinjaRule { rule, name }))
                .unwrap_or_else(|| format!("<missing rule: {}>", build.rule));

            let conflicing_formatted: Vec<String> = conflicting_outputs
                .iter()
                .map(|output| format!("{}", output))
                .collect();

            return Err(format!(
                "different builds generating same file:\n\noutputs:\n - {}\n\nexisting rules:\n - {}\nnew rule:\n - {}\n\nexisting rule definitions:\n{}new rule definition:\n{}\nexisting:\n - {}\nnew:\n - {}",
                conflicing_formatted.join(" - "),
                existing_rules.join("\n - "),
                build.rule,
                existing_rule_defs.join(""),
                new_rule_def,
                existing_conflicts.join(" - "),
                build
            ));
        }

        for output in build.outputs.iter() {
            self.build_outputs.insert(output.clone());
        }
        let build_ref = build.build_ref();
        self.builds.insert(build);
        Ok(build_ref)
    }

    pub fn add_alias(&mut self, name: impl ToString, inputs: Vec<VirtPath>) -> &mut Self {
        let name = name.to_string();
        debug!(
            "alias {}: phony {}",
            name,
            inputs
                .iter()
                .map(|input| input.to_path_buf().display().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        );

        self.aliases.entry(name).or_default().extend(inputs);
        self
    }

    pub fn get_rule_ref(&self, rule: &NinjaRule) -> Option<NinjaRuleRef> {
        self.rules.get(rule).map(|name| NinjaRuleRef(name.clone()))
    }

    pub fn set_build_default(&mut self, build_ref: &NinjaBuildRef) -> &mut Self {
        debug!(
            "default {}",
            build_ref
                .0
                .iter()
                .map(|outp| outp.to_path_buf().display().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        );

        for outp in build_ref.0.iter() {
            self.default_targets.insert(outp.clone());
        }
        self
    }

    pub fn validate(&self) -> Vec<String> {
        // TODO: Better interface than returing string of messages

        let mut errors: BTreeSet<String> = BTreeSet::new();
        let mut output_set = HashSet::new();
        for build in self.builds.iter() {
            for output in build.outputs.iter() {
                let fs_path = output.to_path_buf();
                if !output_set.insert(fs_path.clone()) {
                    errors.insert(format!("Multiple builds generating: {}", fs_path.display()));
                }
            }
        }
        errors.into_iter().collect()
    }
}

/*
 * Tests
 */

#[cfg(test)]
mod tests {
    use super::*;

    type TestF = i32;

    fn test_rule(name: &str, rule: &NinjaRule) -> String {
        format!("{}", NamedNinjaRule { rule, name })
    }

    fn test_path(name: &str) -> VirtPath {
        VirtPath::new("root").step::<TestF>(name).unwrap()
    }

    macro_rules! lines (
        ($line:expr) => ($line);
        ($line:expr, $($rest:expr),+) => (concat!($line, "\n", lines!($($rest),+)));
        () => ("");
    );

    #[test]
    fn test_write_rule() {
        let mut rule = NinjaRule::new();
        rule.var("deps", vec!["boll".into(), "something".into()])
            .var(
                "something",
                vec!["stuff".into(), "stuff".into(), NinjaArg::Var("in".into())],
            );
        assert_eq!(
            test_rule("test", &rule).as_str(),
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
        let mut rule = NinjaRule::new();
        rule.var("a b", vec!["a$b".into(), "a b".into()])
            .var("b", vec!["a\nb".into(), "a:b".into()]);
        assert_eq!(
            test_rule("r$a", &rule).as_str(),
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
        let mut rule = NinjaRule::new();
        rule.var("a b", vec!["a$b".into(), "a b".into()])
            .var("b", vec!["a\nb".into(), "a:b".into()]);
        let mut build = NinjaBuild::new(&NinjaRuleRef("r$a".into()));
        build
            .input(test_path("boll"))
            .input(test_path("hej"))
            .output(test_path("dest"))
            .output(test_path("destb"))
            .var("tjo", vec!["xx".into()]);
        let output = format!("{}{}", test_rule("r$a", &rule), build);
        assert_eq!(
            output.as_str(),
            lines! {
                "rule r$$a",
                "  a$ b = a$$b a$ b",
                "  b = a$",
                "    b a$:b",
                "",
                "build ./dest ./destb: r$$a ./boll ./hej",
                "  tjo = xx",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_build_deps() {
        let rule = NinjaRule::new();
        let mut build = NinjaBuild::new(&NinjaRuleRef("rule".into()));
        build
            .input(test_path("in"))
            .output(test_path("out"))
            .dep(test_path("dep"));
        let output = format!("{}{}", test_rule("rule", &rule), build);
        assert_eq!(
            output.as_str(),
            lines! {
                "rule rule",
                "",
                "build ./out: rule ./in | ./dep",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_build_default() {
        let mut file = NinjaFile::new();
        let rule = file.add_rule(NinjaRule::new());
        let mut build = NinjaBuild::new(&rule);
        build.input(test_path("in")).output(test_path("out"));
        let build_ref = file.add_build(build).unwrap();
        file.set_build_default(&build_ref);
        let output = format!("{}", file);
        assert_eq!(
            output.as_str(),
            lines! {
                "rule rule",
                "",
                "build ./out: rule ./in",
                "",
                "default ./out",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_file() {
        let mut file = NinjaFile::new();

        let mut rule1 = NinjaRule::new();
        rule1.var("x", vec!["stuff".into()]);
        let rule1 = file.add_rule(rule1);

        let mut rule2 = NinjaRule::new();
        rule2.var("y", vec!["stuff".into()]);
        let _rule2 = file.add_rule(rule2);

        let mut build = NinjaBuild::new(&rule1);
        build
            .input(test_path("in1_1"))
            .input(test_path("in1_2"))
            .output(test_path("out1"));
        file.add_build(build).unwrap();

        assert_eq!(
            format!("{}", file),
            lines! {
                "rule x",
                "  x = stuff",
                "",
                "rule y",
                "  y = stuff",
                "",
                "build ./out1: x ./in1_1 ./in1_2",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_file_unique_rules() {
        let mut file = NinjaFile::new();

        let r1 = file.add_rule(NinjaRule::new());
        let r2 = file.add_rule(NinjaRule::new());
        let r3 = file.add_rule(NinjaRule::new());

        let mut r4 = NinjaRule::new();
        r4.var("v", vec!["1".into()]);
        let r4 = file.add_rule(r4);

        let mut r5 = NinjaRule::new();
        r5.var("v", vec!["2".into()]);
        let r5 = file.add_rule(r5);

        assert_eq!(r1, NinjaRuleRef("rule".into()));
        assert_eq!(r2, NinjaRuleRef("rule".into()));
        assert_eq!(r3, NinjaRuleRef("rule".into()));
        assert_eq!(r4, NinjaRuleRef("v".into()));
        assert_eq!(r5, NinjaRuleRef("v1".into()));
    }

    #[test]
    fn test_ref_unique_name() {
        let mut file = NinjaFile::new();

        let _rule = file.add_rule(NinjaRule::new());

        let mut rule1 = NinjaRule::new();
        rule1.var("kind", vec!["one".into()]);
        let rule1 = file.add_rule(rule1);

        let mut rule2 = NinjaRule::new();
        rule2.var("kind", vec!["two".into()]);
        let rule2 = file.add_rule(rule2);

        let mut build1 = NinjaBuild::new(&rule1);
        build1.output(test_path("out1"));
        let build1_ref = file.add_build(build1).unwrap();
        file.set_build_default(&build1_ref);

        let mut build2 = NinjaBuild::new(&rule2);
        build2.output(test_path("out2"));
        file.add_build(build2).unwrap();

        assert_eq!(file.validate(), Vec::<String>::new());

        assert_eq!(
            format!("{}", file),
            lines! {
                "rule kind",
                "  kind = one",
                "",
                "rule kind1",
                "  kind = two",
                "",
                "rule rule",
                "",
                "build ./out1: kind",
                "",
                "build ./out2: kind1",
                "",
                "default ./out1",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_variable_output_name() {
        let mut file = NinjaFile::new();
        file.add_alias("out1", vec![test_path("real_out")]);

        assert_eq!(format!("{}", file), "build out1: phony ./real_out\n\n");
    }

    #[test]
    fn test_aliases_are_grouped_in_file() {
        let mut file = NinjaFile::new();
        file.add_alias("bundle", vec![test_path("first")]);
        file.add_alias("bundle", vec![test_path("second")]);
        file.add_alias("app", vec![test_path("binary")]);

        assert_eq!(
            format!("{}", file),
            lines! {
                "build app: phony ./binary",
                "",
                "build bundle: phony ./first ./second",
                "",
                ""
            }
        );
    }

    #[test]
    fn test_multiple_same_targets() {
        let mut file = NinjaFile::new();
        let rule = file.add_rule(NinjaRule::new());
        let mut build1 = NinjaBuild::new(&rule);
        build1.output(test_path("file"));
        file.add_build(build1).unwrap();

        let mut build2 = NinjaBuild::new(&rule);
        build2.output(test_path("file"));
        assert_eq!(
            file.add_build(build2),
            Ok(NinjaBuildRef(vec![test_path("file")]))
        );

        assert_eq!(file.validate(), Vec::<String>::new());
    }

    #[test]
    fn test_conflicting_targets() {
        let mut file = NinjaFile::new();
        let mut rule1 = NinjaRule::new();
        rule1.var("kind", vec!["one".into()]);
        let rule1 = file.add_rule(rule1);

        let mut rule2 = NinjaRule::new();
        rule2.var("kind", vec!["two".into()]);
        let rule2 = file.add_rule(rule2);

        let mut build1 = NinjaBuild::new(&rule1);
        build1.output(test_path("file"));
        file.add_build(build1).unwrap();

        let mut build2 = NinjaBuild::new(&rule2);
        build2.output(test_path("file"));
        let err = file.add_build(build2).unwrap_err();
        assert!(err.contains("different builds generating same file"));
        assert!(err.contains("./file"));
        assert!(err.contains("existing rules:\n - kind"));
        assert!(err.contains("new rule:\n - kind1"));
        assert!(err.contains("existing rule definitions:\nrule kind\n  kind = one\n\n"));
        assert!(err.contains("new rule definition:\nrule kind1\n  kind = two\n\n"));
        assert!(err.contains("build ./file: kind"));
        assert!(err.contains("build ./file: kind1"));
    }
}
