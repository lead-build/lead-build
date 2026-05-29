use crate::{ast::AstValue, error::Result};
use pest::{
    Parser,
    iterators::{Pair, Pairs},
};
use std::{collections::BTreeMap, fs, path::PathBuf};

use pest_derive::Parser;
#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct Grammar;

macro_rules! visit {
    ( $rule:ident : $func:ident @ $var:ident -> $result:ty $code:block ) => {
        fn $func(node: Pair<Rule>) -> Result<$result> {
            assert!(node.as_rule() == Rule::$rule);
            #[allow(unused_mut)]
            let mut $var: Pairs<Rule> = node.into_inner();
            $code
        }
    };
    ( $rule:ident : $func:ident @ $var:ident -> $result:ty $code:block $( $tail:tt )+ ) => {
        visit! {$rule : $func @ $var -> $result $code}
        visit! { $( $tail )+ }
    }
}

macro_rules! terminals {
    ( $rule:ident : $func:ident ($content:ident) -> $result:ty $code:block ) => {
        fn $func(node: Pair<Rule>) -> Result<$result> {
            assert!(node.as_rule() == Rule::$rule);
            let $content = node.as_str();
            $code
        }
    };
    ( $rule:ident : $func:ident ($content:ident) -> $result:ty $code:block $($tail:tt)+ ) => {
        terminals! { $rule: $func ($content) -> $result $code }
        terminals! { $( $tail )* }
    };
}

macro_rules! next {
    ( $var:ident ) => {
        $var.next().unwrap() as Pair<Rule>
    };
}

pub type Error = pest::error::Error<Rule>;

pub fn parse_file(path: PathBuf) -> Result<AstValue> {
    let content = fs::read_to_string(path)?;
    let parse_tree = Grammar::parse(Rule::entry, &content)?.next().unwrap();
    let ast = visit_expr(parse_tree)?;
    Ok(ast)
}

visit! {
    expr: visit_expr @node -> AstValue {
        let next = next!(node);
        match next.as_rule() {
            Rule::object => Ok(AstValue::Object(visit_object(next)?)),
            Rule::const_int => Ok(AstValue::ConstInt(visit_const_int(next)?)),
            err_rule => panic!("Internal parse error, got {:?}", err_rule),
        }
    }

    object: visit_object @node -> BTreeMap<String, AstValue> {
        let mut fields = BTreeMap::new();
        for assign in node {
            let (name, value) = visit_assignment(assign)?;
            fields.insert(name, value);
        }
        Ok(fields)
    }

    assignment: visit_assignment @node -> (String, AstValue) {
        let name = visit_ident(next!(node))?;
        let value = visit_expr(next!(node))?;
        Ok((name, value))
    }
}

terminals! {
    ident: visit_ident(content) -> String {
        Ok(content.into())
    }

    const_int: visit_const_int(content) -> i64 {
        Ok(content.parse().expect("Internal parse int error"))
    }
}
