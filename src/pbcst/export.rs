use super::{
    Assignment, Attr, BinaryOp, Delimited, Key, LetBinding, MapKind, Matcher, MatcherKind,
    ObjectMatcher, PbNode, PbNodeKind, SwitchCase, UnaryOp,
};
use std::io::{self, Write};

pub trait Exportable {
    fn export<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: Write;

    fn to_source(&self) -> String {
        let mut output = Vec::new();
        self.export(&mut output)
            .expect("writing to a byte vector cannot fail");
        String::from_utf8(output).expect("CST source is always valid UTF-8")
    }
}

impl Exportable for PbNode {
    fn export<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: Write,
    {
        if let Some(source) = &self.source {
            return writer.write_all(source.as_bytes());
        }

        match &self.kind {
            PbNodeKind::Group(value) => {
                writer.write_all(b"(")?;
                value.export(writer)?;
                writer.write_all(b")")
            }
            PbNodeKind::Let(bindings, body) => {
                writer.write_all(b"let ")?;
                for binding in bindings {
                    binding.export(writer)?;
                }
                writer.write_all(b"in ")?;
                body.export(writer)
            }
            PbNodeKind::Bind(assignments, body) => {
                writer.write_all(b"bind ")?;
                for assignment in assignments {
                    assignment.export(writer)?;
                }
                writer.write_all(b"in ")?;
                body.export(writer)
            }
            PbNodeKind::FuncDef(matchers, body) => {
                writer.write_all(b"|")?;
                for (index, matcher) in matchers.iter().enumerate() {
                    if index > 0 {
                        writer.write_all(b" ")?;
                    }
                    matcher.export(writer)?;
                }
                writer.write_all(b"|")?;
                body.export(writer)
            }
            PbNodeKind::Binary(op, lhs, rhs) => {
                lhs.export(writer)?;
                op.export(writer)?;
                rhs.export(writer)
            }
            PbNodeKind::Unary(op, rhs) => {
                op.export(writer)?;
                rhs.export(writer)
            }
            PbNodeKind::FuncCall(func, arg) => {
                func.export(writer)?;
                writer.write_all(b" ")?;
                arg.export(writer)
            }
            PbNodeKind::AttrSel(lhs, attr) => {
                lhs.export(writer)?;
                writer.write_all(b".")?;
                attr.export(writer)
            }
            PbNodeKind::Fold { func, init, input } => {
                writer.write_all(b"(")?;
                func.export(writer)?;
                writer.write_all(b" for ")?;
                if let Some(init) = init {
                    init.export(writer)?;
                    writer.write_all(b":")?;
                }
                input.export(writer)?;
                writer.write_all(b")")
            }
            PbNodeKind::Map {
                kind,
                func,
                input,
                filter,
            } => {
                writer.write_all(match kind {
                    MapKind::List => b"[",
                    MapKind::Object => b"{",
                })?;
                func.export(writer)?;
                writer.write_all(b" for ")?;
                input.export(writer)?;
                if let Some(filter) = filter {
                    writer.write_all(b" if ")?;
                    filter.export(writer)?;
                }
                writer.write_all(match kind {
                    MapKind::List => b"]",
                    MapKind::Object => b"}",
                })
            }
            PbNodeKind::Switch {
                input,
                cases,
                default,
            } => {
                writer.write_all(b"switch ")?;
                input.export(writer)?;
                writer.write_all(b" {")?;
                for case in cases {
                    case.export(writer)?;
                }
                if let Some(default) = default {
                    writer.write_all(b"_=>")?;
                    default.export(writer)?;
                    writer.write_all(b";")?;
                }
                writer.write_all(b"}")
            }
            PbNodeKind::Object(items) => {
                writer.write_all(b"{")?;
                for item in items {
                    item.export(writer)?;
                }
                writer.write_all(b"}")
            }
            PbNodeKind::List(items) => export_delimited(writer, b"[", b"]", items),
            PbNodeKind::Tuple(items) => export_delimited(writer, b"(", b")", items),
            PbNodeKind::Bool(value) => write!(writer, "{value}"),
            PbNodeKind::Int(value) | PbNodeKind::String(value) => {
                writer.write_all(value.as_bytes())
            }
            PbNodeKind::Var(name) => write!(writer, "{name}"),
            PbNodeKind::Null => writer.write_all(b"null"),
        }
    }
}

impl Exportable for BinaryOp {
    fn export<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: Write,
    {
        writer.write_all(match self {
            Self::HasAttr => b"?",
            Self::ListConcat => b"++",
            Self::Mult => b"*",
            Self::Div => b"/",
            Self::Sub => b"-",
            Self::Add => b"+",
            Self::Update => b"//",
            Self::Lt => b"<",
            Self::Le => b"<=",
            Self::Gt => b">",
            Self::Ge => b">=",
            Self::Eq => b"==",
            Self::Neq => b"!=",
            Self::LogAnd => b"&&",
            Self::LogOr => b"||",
            Self::LogImpl => b"->",
        })
    }
}

impl Exportable for UnaryOp {
    fn export<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: Write,
    {
        writer.write_all(match self {
            Self::Neg => b"-",
            Self::Not => b"!",
        })
    }
}

impl Exportable for Attr {
    fn export<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: Write,
    {
        match self {
            Attr::Ident(name, _) => write!(writer, "{name}"),
            Attr::Dynamic(value) => {
                writer.write_all(b"{")?;
                value.export(writer)?;
                writer.write_all(b"}")
            }
        }
    }
}

impl Exportable for Matcher {
    fn export<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: Write,
    {
        match &self.kind {
            MatcherKind::Alias(matcher, name, _) => {
                matcher.export(writer)?;
                write!(writer, "@{name}")
            }
            MatcherKind::DontCare => writer.write_all(b"_"),
            MatcherKind::Ident(name) => write!(writer, "{name}"),
            MatcherKind::Tuple(items) => export_delimited(writer, b"(", b")", items),
            MatcherKind::Object { fields, exhaustive } => {
                writer.write_all(b"{")?;
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        writer.write_all(b",")?;
                    }
                    field.export(writer)?;
                }
                if !exhaustive {
                    if !fields.is_empty() {
                        writer.write_all(b",")?;
                    }
                    writer.write_all(b"...")?;
                }
                writer.write_all(b"}")
            }
        }
    }
}

impl Exportable for LetBinding {
    fn export<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: Write,
    {
        self.matcher.export(writer)?;
        writer.write_all(b"=")?;
        self.value.export(writer)?;
        writer.write_all(b";")
    }
}

impl Exportable for Assignment {
    fn export<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: Write,
    {
        self.key.export(writer)?;
        writer.write_all(b"=")?;
        self.value.export(writer)?;
        writer.write_all(b";")
    }
}

impl Exportable for Key {
    fn export<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: Write,
    {
        match self {
            Key::Ident(key, _) => write!(writer, "{key}"),
            Key::String(key, _) => writer.write_all(key.as_bytes()),
        }
    }
}

impl Exportable for SwitchCase {
    fn export<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: Write,
    {
        self.matcher.export(writer)?;
        writer.write_all(b"=>")?;
        self.value.export(writer)?;
        writer.write_all(b";")
    }
}

impl Exportable for ObjectMatcher {
    fn export<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: Write,
    {
        write!(writer, "{}", self.key)?;
        if !matches!(&self.matcher.kind, MatcherKind::Ident(name) if *name == self.key) {
            writer.write_all(b"=")?;
            self.matcher.export(writer)?;
        }
        if let Some(default) = &self.default {
            writer.write_all(b"?")?;
            default.export(writer)?;
        }
        Ok(())
    }
}

fn export_delimited<W, T>(
    writer: &mut W,
    open: &[u8],
    close: &[u8],
    delimited: &Delimited<T>,
) -> io::Result<()>
where
    W: Write,
    T: Exportable,
{
    writer.write_all(open)?;
    for (index, item) in delimited.items.iter().enumerate() {
        if index > 0 {
            writer.write_all(b",")?;
        }
        item.export(writer)?;
    }
    if delimited.trailing {
        writer.write_all(b",")?;
    }
    writer.write_all(close)
}
