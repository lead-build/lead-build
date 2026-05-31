use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Clone)]
pub struct ImMap<T: Clone + PartialEq>(BTreeMap<String, T>);

#[derive(Debug)]
pub enum Error {
    DupKey(String),
}

impl ToString for Error {
    fn to_string(&self) -> String {
        match self {
            Error::DupKey(key) => format!("Duplicate key: {}", key),
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

impl<T> Default for ImMap<T>
where
    T: Clone + PartialEq,
{
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T> ImMap<T>
where
    T: Clone + PartialEq,
{
    pub fn new() -> ImMap<T> {
        Default::default()
    }

    pub fn from(fields: impl Iterator<Item = (String, T)>) -> Result<ImMap<T>> {
        let mut ret: ImMap<T> = Default::default();
        for (key, value) in fields {
            ret = ret.set_inplace(key, value)?
        }
        Ok(ret)
    }

    pub fn set_inplace(self, key: String, value: T) -> Result<ImMap<T>> {
        let mut map = self;
        let res = map.0.insert(key.clone(), value);
        match res {
            Some(_) => Err(Error::DupKey(key)),
            None => Ok(map),
        }
    }

    pub fn set(&self, key: String, value: T) -> Option<ImMap<T>> {
        let mut map = self.0.clone();
        map.insert(key, value)?;
        Some(ImMap(map))
    }

    pub fn get(&self, key: &str) -> Option<T> {
        self.0.get(key).cloned()
    }
}
