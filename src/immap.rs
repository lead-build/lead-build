use std::{collections::BTreeMap, fmt::Display};

#[derive(Debug)]
pub enum Error {
    DupKey(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::DupKey(key) => write!(f, "Duplicate key: {}", key),
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, PartialEq, Clone)]
pub struct ImMap<T: Display + Clone + PartialEq>(BTreeMap<String, T>);

impl<T> Display for ImMap<T>
where
    T: Clone + PartialEq + Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("{ ")?;
        for (key, value) in self.0.iter() {
            f.write_str(key)?;
            f.write_str(" = ")?;
            value.fmt(f)?;
            f.write_str("; ")?;
        }
        f.write_str("}")?;
        Ok(())
    }
}

impl<T> Default for ImMap<T>
where
    T: Display + Clone + PartialEq,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ImMap<T>
where
    T: Display + Clone + PartialEq,
{
    pub fn new() -> ImMap<T> {
        Self(BTreeMap::new())
    }

    pub fn single(key: impl ToString, value: T) -> ImMap<T> {
        Self::new().set(key.to_string(), value).unwrap()
    }

    pub fn from(fields: impl IntoIterator<Item = (impl ToString, T)>) -> Result<ImMap<T>> {
        let mut ret: ImMap<T> = Default::default();
        for (key, value) in fields {
            ret = ret.set(key.to_string(), value)?
        }
        Ok(ret)
    }

    pub fn merge(self, other: &ImMap<T>) -> ImMap<T> {
        let mut out = self.0;
        let mut to_update = other.0.clone();
        out.append(&mut to_update);
        ImMap(out)
    }

    pub fn set(self, key: impl ToString, value: T) -> Result<ImMap<T>> {
        let mut map = self;
        let res = map.0.insert(key.to_string(), value);
        match res {
            Some(_) => Err(Error::DupKey(key.to_string())),
            None => Ok(map),
        }
    }

    pub fn unset(self, key: &str) -> ImMap<T> {
        let mut map = self;
        map.0.remove(key);
        map
    }

    pub fn get(&self, key: &str) -> Option<T> {
        self.0.get(key).cloned()
    }

    pub fn map<B, F>(&self, f: F) -> ImMap<B>
    where
        F: Fn(&T) -> B,
        B: Display + Clone + PartialEq,
    {
        ImMap::from(self.0.iter().map(|(name, value)| (name.clone(), f(value)))).unwrap()
    }
}
