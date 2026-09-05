use super::*;
use crate::pblang::{
    Assignment, Attr, BinaryOp, Delimited, Key, LetBinding, MapKind, Matcher as PbMatcher,
    MatcherKind as PbMatcherKind, ObjectMatcher, PbNode, PbNodeKind, SwitchCase, UnaryOp,
};

pub type ExportResult<T> = std::result::Result<T, ExportError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportError(pub String);

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExportError {}

pub trait Exportable {
    fn export(&self) -> ExportResult<PbNode>;
}

impl<T, F> Exportable for Expr<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    fn export(&self) -> ExportResult<PbNode> {
        self.inner_ref().tok.export()
    }
}

impl<T, F> Exportable for ExprType<T, F>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    fn export(&self) -> ExportResult<PbNode> {
        let kind = match self {
            ExprType::Object(items) => PbNodeKind::Object(
                items
                    .iter()
                    .map(|(key, value)| {
                        Ok(Assignment {
                            key: Key::Ident(*key, None),
                            value: value.export()?,
                            span: None,
                        })
                    })
                    .collect::<ExportResult<_>>()?,
            ),
            ExprType::List(items) => PbNodeKind::List(Delimited {
                items: items
                    .iter()
                    .map(Exportable::export)
                    .collect::<ExportResult<_>>()?,
                trailing: true,
            }),
            ExprType::Tuple(items) => PbNodeKind::Tuple(Delimited {
                items: items
                    .iter()
                    .map(Exportable::export)
                    .collect::<ExportResult<_>>()?,
                trailing: true,
            }),
            ExprType::Concat(_) => todo!("exporting multipart strings to CST"),
            ExprType::AttrSel(value, attr) => PbNodeKind::AttrSel(
                Box::new(value.export()?),
                match &attr.inner_ref().tok {
                    ExprType::Value(value) if is_ident(&value.to_string()) => {
                        Attr::Ident(StrKey::from(&value.to_string()), None)
                    }
                    _ => Attr::Dynamic(Box::new(attr.export()?)),
                },
            ),
            ExprType::Value(value) => return value.export(),
            ExprType::Var(name) => PbNodeKind::Var(*name),
            ExprType::UnOp(op, value) => PbNodeKind::Unary(
                match op {
                    ExprUnOp::Neg => UnaryOp::Neg,
                    ExprUnOp::Not => UnaryOp::Not,
                },
                Box::new(value.export()?),
            ),
            ExprType::BinOp(op, lhs, rhs) => PbNodeKind::Binary(
                binary_op(*op),
                Box::new(lhs.export()?),
                Box::new(rhs.export()?),
            ),
            ExprType::FuncDef(matcher, body) => {
                PbNodeKind::FuncDef(vec![export_matcher(matcher)?], Box::new(body.export()?))
            }
            ExprType::FuncDefBuiltin(_) => {
                return Err(ExportError("builtin functions cannot be exported".into()));
            }
            ExprType::Let(bindings, body) => PbNodeKind::Let(
                bindings
                    .iter()
                    .map(|(matcher, value)| {
                        Ok(LetBinding {
                            matcher: export_matcher(matcher)?,
                            value: value.export()?,
                            span: None,
                        })
                    })
                    .collect::<ExportResult<_>>()?,
                Box::new(body.export()?),
            ),
            ExprType::Fold { func, init, input } => PbNodeKind::Fold {
                func: Box::new(func.export()?),
                init: init
                    .as_ref()
                    .map(Exportable::export)
                    .transpose()?
                    .map(Box::new),
                input: Box::new(input.export()?),
            },
            ExprType::Map(kind, func, input, filter) => PbNodeKind::Map {
                kind: match kind {
                    ExprMapType::List => MapKind::List,
                    ExprMapType::Object => MapKind::Object,
                },
                func: Box::new(func.export()?),
                input: Box::new(input.export()?),
                filter: filter
                    .as_ref()
                    .map(Exportable::export)
                    .transpose()?
                    .map(Box::new),
            },
            ExprType::FuncCall { arg, func } => {
                PbNodeKind::FuncCall(Box::new(func.export()?), Box::new(arg.export()?))
            }
            ExprType::Bind(items, body) => PbNodeKind::Bind(
                items
                    .iter()
                    .map(|(key, value)| {
                        Ok(Assignment {
                            key: Key::Ident(*key, None),
                            value: value.export()?,
                            span: None,
                        })
                    })
                    .collect::<ExportResult<_>>()?,
                Box::new(body.export()?),
            ),
            ExprType::Switch(input, cases, default) => PbNodeKind::Switch {
                input: Box::new(input.export()?),
                cases: cases
                    .iter()
                    .map(|(matcher, value)| {
                        Ok(SwitchCase {
                            matcher: matcher.export()?,
                            value: value.export()?,
                            span: None,
                        })
                    })
                    .collect::<ExportResult<_>>()?,
                default: default
                    .as_ref()
                    .map(Exportable::export)
                    .transpose()?
                    .map(Box::new),
            },
            ExprType::Null => PbNodeKind::Null,
            ExprType::UnderEval => panic!("exporting an expression under evaluation"),
        };
        Ok(PbNode::generated(kind))
    }
}

fn export_matcher<T, F>(matcher: &Matcher<T, F>) -> ExportResult<PbMatcher>
where
    T: Clone + PartialEq + Display + ExprOps<F> + Debug + Exportable,
    F: Clone + Debug + Referrable,
{
    let kind = match matcher {
        Matcher::Alias(matcher, name) => {
            PbMatcherKind::Alias(Box::new(export_matcher(matcher)?), *name, None)
        }
        Matcher::DontCare => PbMatcherKind::DontCare,
        Matcher::Ident(name) => PbMatcherKind::Ident(*name),
        Matcher::Tuple(items) => PbMatcherKind::Tuple(Delimited {
            items: items
                .iter()
                .map(export_matcher)
                .collect::<ExportResult<_>>()?,
            trailing: true,
        }),
        Matcher::Object(items, exhaustive) => PbMatcherKind::Object {
            fields: items
                .iter()
                .map(|(key, matcher, default)| {
                    Ok(ObjectMatcher {
                        key: *key,
                        matcher: export_matcher(matcher)?,
                        default: default.as_ref().map(Exportable::export).transpose()?,
                        span: None,
                    })
                })
                .collect::<ExportResult<_>>()?,
            exhaustive: *exhaustive,
        },
    };
    Ok(PbMatcher { kind, span: None })
}

fn binary_op(op: ExprBinOp) -> BinaryOp {
    match op {
        ExprBinOp::HasAttr => BinaryOp::HasAttr,
        ExprBinOp::ListConcat => BinaryOp::ListConcat,
        ExprBinOp::Mult => BinaryOp::Mult,
        ExprBinOp::Div => BinaryOp::Div,
        ExprBinOp::Sub => BinaryOp::Sub,
        ExprBinOp::Add => BinaryOp::Add,
        ExprBinOp::Update => BinaryOp::Update,
        ExprBinOp::Lt => BinaryOp::Lt,
        ExprBinOp::Le => BinaryOp::Le,
        ExprBinOp::Gt => BinaryOp::Gt,
        ExprBinOp::Ge => BinaryOp::Ge,
        ExprBinOp::Eq => BinaryOp::Eq,
        ExprBinOp::Neq => BinaryOp::Neq,
        ExprBinOp::LogAnd => BinaryOp::LogAnd,
        ExprBinOp::LogOr => BinaryOp::LogOr,
        ExprBinOp::LogImpl => BinaryOp::LogImpl,
    }
}

fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
