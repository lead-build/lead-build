use std::collections::BTreeMap;

#[derive(Debug)]
pub enum AstValue {
    Object(BTreeMap<String, AstValue>),
    ConstInt(i64),
}
