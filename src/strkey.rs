use std::{
    collections::BTreeMap,
    fmt::{self, Display},
    sync::{Mutex, MutexGuard, OnceLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StrKey(usize);

struct StrKeyPool {
    str_to_idx: BTreeMap<String, usize>,
    idx_to_str: Vec<String>,
}

static STRKEY_POOL: OnceLock<Mutex<StrKeyPool>> = OnceLock::new();

fn strkey_pool() -> &'static Mutex<StrKeyPool> {
    STRKEY_POOL.get_or_init(|| {
        Mutex::new(StrKeyPool {
            str_to_idx: BTreeMap::new(),
            idx_to_str: Vec::new(),
        })
    })
}

// Waits for the lock regardless of poisoning, so one thread's panic can't
// make every other thread panic instead of just blocking as usual.
fn lock_pool() -> MutexGuard<'static, StrKeyPool> {
    strkey_pool()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl StrKeyPool {
    fn get(&mut self, s: &str) -> usize {
        if let Some(&existing) = self.str_to_idx.get(s) {
            existing
        } else {
            let new_str = s.to_string();
            let new_index = self.idx_to_str.len();
            self.idx_to_str.push(new_str.clone());
            self.str_to_idx.insert(new_str, new_index);
            new_index
        }
    }
}

impl StrKey {
    pub fn as_string(&self) -> String {
        let pool = lock_pool();
        if self.0 < pool.idx_to_str.len() {
            pool.idx_to_str[self.0].clone()
        } else {
            panic!("Resolving invalid StrKey index: {}", self.0);
        }
    }

    pub fn get_raw_id(&self) -> usize {
        self.0
    }

    pub fn from_raw_id(id: usize) -> Self {
        StrKey(id)
    }
}

impl Display for StrKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_string())
    }
}

impl From<&str> for StrKey {
    fn from(s: &str) -> Self {
        StrKey(lock_pool().get(s))
    }
}

impl From<&String> for StrKey {
    fn from(s: &String) -> Self {
        StrKey::from(s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_string_returns_same_key() {
        let a = StrKey::from("test_same_string_returns_same_key/foo");
        let b = StrKey::from("test_same_string_returns_same_key/foo");
        assert_eq!(a, b);
    }

    #[test]
    fn test_different_strings_return_different_keys() {
        let a = StrKey::from("test_different_strings_return_different_keys/foo");
        let b = StrKey::from("test_different_strings_return_different_keys/bar");
        assert_ne!(a, b);
    }

    #[test]
    fn test_strkey_to_string_round_trip() {
        let key = StrKey::from("test_strkey_to_string_round_trip/baz");
        assert_eq!(
            key.as_string(),
            "test_strkey_to_string_round_trip/baz".to_string()
        );
    }

    #[test]
    #[should_panic]
    fn test_strkey_to_string_out_of_range() {
        StrKey(usize::MAX).as_string();
    }
}
