use crate::ast::AstValue;
use pest_consume::{Parser, match_nodes};
use std::{collections::BTreeMap, fs, num::ParseIntError, path::PathBuf};

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct DnjParser;

pub type Error = pest_consume::Error<Rule>;
pub type Result<T> = std::result::Result<T, Error>;
type Node<'i> = pest_consume::Node<'i, Rule, ()>;

#[pest_consume::parser]
impl DnjParser {
    fn EOI(_input: Node) -> Result<()> {
        Ok(())
    }

    fn entry(input: Node) -> Result<AstValue> {
        Ok(match_nodes! {input.into_children();
            [expr(e), EOI(_)] => e,
        })
    }

    fn expr(input: Node) -> Result<AstValue> {
        Ok(match_nodes! {input.into_children();
            [object(o)] => AstValue::Object(o),
            [const_int(i)] => AstValue::ConstInt(i),
        })
    }

    fn object(input: Node) -> Result<BTreeMap<String, AstValue>> {
        let assignments = match_nodes! {input.into_children();
            [assignment(a)..] => a
        };
        let mut fields = BTreeMap::new();
        for (name, value) in assignments {
            fields.insert(name, value);
        }
        Ok(fields)
    }

    fn assignment(input: Node) -> Result<(String, AstValue)> {
        Ok(match_nodes! {input.into_children();
            [ident(ident), expr(val)] => (ident, val),
        })
    }

    fn ident(input: Node) -> Result<String> {
        Ok(input.as_str().into())
    }

    fn const_int(input: Node) -> Result<i64> {
        Ok(input
            .as_str()
            .parse()
            .map_err(|e: ParseIntError| input.error(e.to_string()))?)
    }
}

impl DnjParser {
    pub fn parse_file(path: PathBuf) -> Result<AstValue> {
        let content = fs::read_to_string(path).unwrap();
        let parse_tree = DnjParser::parse(Rule::entry, &content)?;
        let input = parse_tree.single()?;
        DnjParser::entry(input)
    }
}
