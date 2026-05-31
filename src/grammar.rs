use crate::datamodel::Expr;
use pest_consume::{Parser, match_nodes};
use std::{collections::BTreeMap, fs, num::ParseIntError, path::PathBuf};

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct DnjParser;

pub type Error = pest_consume::Error<Rule>;
pub type Result<T> = std::result::Result<T, Error>;
type Node<'i> = pest_consume::Node<'i, Rule, ()>;

impl DnjParser {
    pub fn parse_file(path: PathBuf) -> Result<Expr> {
        let input_str = fs::read_to_string(path).unwrap();
        Self::parse_str(&input_str)
    }
    pub fn parse_str(input_str: &str) -> Result<Expr> {
        let parse_tree = DnjParser::parse(Rule::entry, input_str)?;
        let input = parse_tree.single()?;
        DnjParser::entry(input)
    }
}

#[pest_consume::parser]
impl DnjParser {
    fn EOI(_input: Node) -> Result<()> {
        Ok(())
    }

    fn entry(input: Node) -> Result<Expr> {
        Ok(match_nodes! {input.into_children();
            [expr(e), EOI(_)] => e,
        })
    }

    /*
     * Expression
     */

    fn expr(input: Node) -> Result<Expr> {
        Ok(match_nodes! {input.into_children();
            [object(x)] => x,
            [const_int(x)] => x,
            [const_str(x)] => x,
            [func_call(x)] => x,
            [func_def(x)] => x,
            [let_def(x)] => x,
        })
    }

    /*
     * Primitives
     */

    fn object(input: Node) -> Result<Expr> {
        let assignments = match_nodes! {input.into_children();
            [object_assignment(a)..] => a
        };
        let mut fields = BTreeMap::new();
        for (name, value) in assignments {
            fields.insert(name, value);
        }
        Ok(Expr::Object(fields.into()))
    }

    fn object_assignment(input: Node) -> Result<(String, Expr)> {
        Ok(match_nodes! {input.into_children();
            [ident(ident), expr(val)] => (ident, val),
        })
    }

    fn func_call(input: Node) -> Result<Expr> {
        Ok(match_nodes! {input.into_children();
            [ident(ident), expr(val)] => Expr::FuncCall(ident, val.into()),
        })
    }

    /*
     * Function definition
     */

    fn func_def(input: Node) -> Result<Expr> {
        Ok(match_nodes! {input.into_children();
            [ident(ident), expr(val)] => Expr::FuncDefIdent(ident, val.into()),
            [func_args_pattern(pat), expr(val)] => Expr::FuncDefPattern(pat, val.into()),
        })
    }

    fn func_args_pattern(input: Node) -> Result<Vec<String>> {
        Ok(match_nodes! {input.into_children();
            [ident(ident)..] => ident,
        }
        .collect())
    }

    /*
     * Let blocks
     */

    fn let_def(input: Node) -> Result<Expr> {
        let (bl, ex) = match_nodes! {input.into_children();
            [let_block(bl), expr(ex)] => (bl, ex)
        };
        Ok(Expr::Let(bl, ex.into()))
    }

    fn let_block(input: Node) -> Result<Vec<(String, Box<Expr>)>> {
        Ok(match_nodes! {input.into_children();
            [let_stmt(stmt)..] => stmt,
        }
        .collect())
    }

    fn let_stmt(input: Node) -> Result<(String, Box<Expr>)> {
        Ok(match_nodes! {input.into_children();
            [ident(ident), expr(val)] => (ident, val.into()),
        })
    }

    /*
     * Literals
     */

    fn ident(input: Node) -> Result<String> {
        Ok(input.as_str().into())
    }

    fn const_int(input: Node) -> Result<Expr> {
        let value = input
            .as_str()
            .parse()
            .map_err(|e: ParseIntError| input.error(e.to_string()))?;
        Ok(Expr::Int(value))
    }

    fn const_str(input: Node) -> Result<Expr> {
        Ok(Expr::String(
            match_nodes! {input.into_children();
                [const_str_sym(c)..] => c,
            }
            .collect(),
        ))
    }

    fn const_str_sym(input: Node) -> Result<char> {
        Ok(match_nodes! {input.into_children();
            [const_str_char(c)] => c,
            [const_str_esc(c)] => c,
            [const_str_hex(c)] => c,
        })
    }

    fn const_str_char(input: Node) -> Result<char> {
        Ok(input.as_str().chars().next().unwrap())
    }

    fn const_str_esc(input: Node) -> Result<char> {
        let chr = input.as_str().chars().next().unwrap();
        Ok(match chr {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            c => c,
        })
    }

    fn const_str_hex(input: Node) -> Result<char> {
        let str = input.as_str();
        let val = u32::from_str_radix(str, 16).unwrap();
        Ok(char::from_u32(val).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_int() {
        let tree = DnjParser::parse_str("1231").unwrap();
        assert_eq!(Expr::Int(1231), tree);
    }

    #[test]
    fn test_parse_obj() {
        let code = r#"
            {
                boll = 123;
                hej = 323;
            }
        "#;
        let tree = DnjParser::parse_str(code).unwrap();
        assert_eq!(
            Expr::Object(
                BTreeMap::from([
                    ("boll".into(), Expr::Int(123)),
                    ("hej".into(), Expr::Int(323))
                ])
                .into()
            ),
            tree
        );
    }

    #[test]
    fn test_parse_obj_in_obj() {
        let code = r#"
            {
                boll = 123;
                hej = { a=2; b=3; };
            }
        "#;
        let tree = DnjParser::parse_str(code).unwrap();
        assert_eq!(
            Expr::Object(
                BTreeMap::from([
                    ("boll".into(), Expr::Int(123)),
                    (
                        "hej".into(),
                        Expr::Object(
                            BTreeMap::from([
                                ("a".into(), Expr::Int(2)),
                                ("b".into(), Expr::Int(3)),
                            ])
                            .into()
                        )
                    )
                ])
                .into()
            ),
            tree
        );
    }

    #[test]
    fn test_parse_str() {
        let code = "\"boll\\\"hej\\u0041\"";
        let tree = DnjParser::parse_str(code).unwrap();
        assert_eq!(Expr::String("boll\"hejA".into()), tree);
    }

    #[test]
    fn test_parse_func_call() {
        let code = "hej 12";
        let tree = DnjParser::parse_str(code).unwrap();
        assert_eq!(Expr::FuncCall("hej".into(), Expr::Int(12).into()), tree);
    }

    #[test]
    fn test_parse_func_def_ident() {
        let code = "hej: 12";
        let tree = DnjParser::parse_str(code).unwrap();
        assert_eq!(Expr::FuncDefIdent("hej".into(), Expr::Int(12).into()), tree);
    }

    #[test]
    fn test_parse_func_def_pattern1() {
        let code = "{ hej, hopp, svej }: 12";
        let tree = DnjParser::parse_str(code).unwrap();
        assert_eq!(
            Expr::FuncDefPattern(
                vec!["hej".into(), "hopp".into(), "svej".into()],
                Expr::Int(12).into()
            ),
            tree
        );
    }

    #[test]
    fn test_parse_func_def_pattern2() {
        let code = "{ hej, hopp, svej, }: 12";
        let tree = DnjParser::parse_str(code).unwrap();
        assert_eq!(
            Expr::FuncDefPattern(
                vec!["hej".into(), "hopp".into(), "svej".into()],
                Expr::Int(12).into()
            ),
            tree
        );
    }

    #[test]
    fn test_parse_let() {
        let code = "let a = 21; b = 33; in 434";
        let tree = DnjParser::parse_str(code).unwrap();
        assert_eq!(
            Expr::Let(
                vec![
                    ("a".into(), Expr::Int(21).into()),
                    ("b".into(), Expr::Int(33).into()),
                ],
                Expr::Int(434).into(),
            ),
            tree
        );
    }
}
