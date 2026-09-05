use super::{
    Assignment, Attr, BinaryOp, Delimited, Key, LetBinding, MapKind, Matcher, MatcherKind,
    ObjectMatcher, PbNode, PbNodeKind, StringPart, SwitchCase, UnaryOp,
};
use std::fmt::{Result, Write};

pub trait PbLangExportable {
    fn export<W>(&self, writer: &mut W) -> Result
    where
        W: Write;

    fn to_source(&self) -> String {
        let mut output = String::new();
        self.export(&mut output)
            .expect("writing to a string cannot fail");
        output
    }
}

impl PbLangExportable for PbNode {
    fn export<W>(&self, writer: &mut W) -> Result
    where
        W: Write,
    {
        match &self.kind {
            PbNodeKind::Group(value) => {
                writer.write_str("(")?;
                value.export(writer)?;
                writer.write_str(")")
            }
            PbNodeKind::Let(bindings, body) => {
                writer.write_str("let ")?;
                for binding in bindings {
                    binding.export(writer)?;
                }
                writer.write_str("in ")?;
                body.export(writer)
            }
            PbNodeKind::Bind(assignments, body) => {
                writer.write_str("bind ")?;
                for assignment in assignments {
                    assignment.export(writer)?;
                }
                writer.write_str("in ")?;
                body.export(writer)
            }
            PbNodeKind::FuncDef(matchers, body) => {
                writer.write_str("|")?;
                for (index, matcher) in matchers.iter().enumerate() {
                    if index > 0 {
                        writer.write_str(" ")?;
                    }
                    matcher.export(writer)?;
                }
                writer.write_str("|")?;
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
                writer.write_str(" ")?;
                arg.export(writer)
            }
            PbNodeKind::AttrSel(lhs, attr) => {
                lhs.export(writer)?;
                writer.write_str(".")?;
                attr.export(writer)
            }
            PbNodeKind::Fold { func, init, input } => {
                writer.write_str("(")?;
                func.export(writer)?;
                writer.write_str(" for ")?;
                if let Some(init) = init {
                    init.export(writer)?;
                    writer.write_str(":")?;
                }
                input.export(writer)?;
                writer.write_str(")")
            }
            PbNodeKind::Map {
                kind,
                func,
                input,
                filter,
            } => {
                writer.write_str(match kind {
                    MapKind::List => "[",
                    MapKind::Object => "{",
                })?;
                func.export(writer)?;
                writer.write_str(" for ")?;
                input.export(writer)?;
                if let Some(filter) = filter {
                    writer.write_str(" if ")?;
                    filter.export(writer)?;
                }
                writer.write_str(match kind {
                    MapKind::List => "]",
                    MapKind::Object => "}",
                })
            }
            PbNodeKind::Switch {
                input,
                cases,
                default,
            } => {
                writer.write_str("switch ")?;
                input.export(writer)?;
                writer.write_str(" {")?;
                for case in cases {
                    case.export(writer)?;
                }
                if let Some(default) = default {
                    writer.write_str("_=>")?;
                    default.export(writer)?;
                    writer.write_str(";")?;
                }
                writer.write_str("}")
            }
            PbNodeKind::Object(items) => {
                writer.write_str("{")?;
                for item in items {
                    item.export(writer)?;
                }
                writer.write_str("}")
            }
            PbNodeKind::List(items) => export_delimited(writer, "[", "]", items),
            PbNodeKind::Tuple(items) => export_delimited(writer, "(", ")", items),
            PbNodeKind::Bool(value) => write!(writer, "{value}"),
            PbNodeKind::Int(value) => writer.write_str(value),
            PbNodeKind::String(parts) => export_string_parts(writer, parts),
            PbNodeKind::Var(name) => write!(writer, "{name}"),
            PbNodeKind::Null => writer.write_str("null"),
            PbNodeKind::OutputPlaceholder(label) => write!(writer, "<{label}>"),
            PbNodeKind::OutputElided { label, omitted } => {
                write!(writer, "<{label}; {omitted} omitted>")
            }
            PbNodeKind::OutputCycle => writer.write_str("<recursive expression>"),
        }
    }
}

impl PbLangExportable for BinaryOp {
    fn export<W>(&self, writer: &mut W) -> Result
    where
        W: Write,
    {
        writer.write_str(match self {
            Self::HasAttr => "?",
            Self::ListConcat => "++",
            Self::Mult => "*",
            Self::Div => "/",
            Self::Sub => "-",
            Self::Add => "+",
            Self::Update => "//",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Eq => "==",
            Self::Neq => "!=",
            Self::LogAnd => "&&",
            Self::LogOr => "||",
            Self::LogImpl => "->",
        })
    }
}

impl PbLangExportable for UnaryOp {
    fn export<W>(&self, writer: &mut W) -> Result
    where
        W: Write,
    {
        writer.write_str(match self {
            Self::Neg => "-",
            Self::Not => "!",
        })
    }
}

impl PbLangExportable for Attr {
    fn export<W>(&self, writer: &mut W) -> Result
    where
        W: Write,
    {
        match self {
            Attr::Ident(name, _) => write!(writer, "{name}"),
            Attr::Dynamic(value) => {
                writer.write_str("{")?;
                value.export(writer)?;
                writer.write_str("}")
            }
        }
    }
}

impl PbLangExportable for Matcher {
    fn export<W>(&self, writer: &mut W) -> Result
    where
        W: Write,
    {
        match &self.kind {
            MatcherKind::Alias(matcher, name, _) => {
                matcher.export(writer)?;
                write!(writer, "@{name}")
            }
            MatcherKind::DontCare => writer.write_str("_"),
            MatcherKind::Ident(name) => write!(writer, "{name}"),
            MatcherKind::Tuple(items) => export_delimited(writer, "(", ")", items),
            MatcherKind::Object { fields, exhaustive } => {
                writer.write_str("{")?;
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        writer.write_str(",")?;
                    }
                    field.export(writer)?;
                }
                if !exhaustive {
                    if !fields.is_empty() {
                        writer.write_str(",")?;
                    }
                    writer.write_str("...")?;
                }
                writer.write_str("}")
            }
        }
    }
}

impl PbLangExportable for LetBinding {
    fn export<W>(&self, writer: &mut W) -> Result
    where
        W: Write,
    {
        self.matcher.export(writer)?;
        writer.write_str("=")?;
        self.value.export(writer)?;
        writer.write_str(";")
    }
}

impl PbLangExportable for Assignment {
    fn export<W>(&self, writer: &mut W) -> Result
    where
        W: Write,
    {
        self.key.export(writer)?;
        writer.write_str("=")?;
        self.value.export(writer)?;
        writer.write_str(";")
    }
}

impl PbLangExportable for Key {
    fn export<W>(&self, writer: &mut W) -> Result
    where
        W: Write,
    {
        match self {
            Key::Ident(key, _) => write!(writer, "{key}"),
            Key::String(parts, _) => export_string_parts(writer, parts),
        }
    }
}

fn export_string_parts<W>(writer: &mut W, parts: &[StringPart]) -> Result
where
    W: Write,
{
    writer.write_str("\"")?;
    for part in parts {
        match part {
            StringPart::Chunk(raw) => writer.write_str(raw)?,
            StringPart::Embed(expr) => {
                writer.write_str("${")?;
                expr.export(writer)?;
                writer.write_str("}")?;
            }
        }
    }
    writer.write_str("\"")
}

impl PbLangExportable for SwitchCase {
    fn export<W>(&self, writer: &mut W) -> Result
    where
        W: Write,
    {
        self.matcher.export(writer)?;
        writer.write_str("=>")?;
        self.value.export(writer)?;
        writer.write_str(";")
    }
}

impl PbLangExportable for ObjectMatcher {
    fn export<W>(&self, writer: &mut W) -> Result
    where
        W: Write,
    {
        write!(writer, "{}", self.key)?;
        if !matches!(&self.matcher.kind, MatcherKind::Ident(name) if *name == self.key) {
            writer.write_str("=")?;
            self.matcher.export(writer)?;
        }
        if let Some(default) = &self.default {
            writer.write_str("?")?;
            default.export(writer)?;
        }
        Ok(())
    }
}

fn export_delimited<W, T>(
    writer: &mut W,
    open: &str,
    close: &str,
    delimited: &Delimited<T>,
) -> Result
where
    W: Write,
    T: PbLangExportable,
{
    writer.write_str(open)?;
    for (index, item) in delimited.items.iter().enumerate() {
        if index > 0 {
            writer.write_str(",")?;
        }
        item.export(writer)?;
    }
    if delimited.trailing {
        writer.write_str(",")?;
    }
    writer.write_str(close)
}
