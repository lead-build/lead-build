use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

use pathdiff::diff_paths;

use crate::lang::{Error, ErrorType, Referrable, Result};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct VirtPath {
    name: String,
    root: PathBuf,
    locked_parts: Vec<String>,
    parts: Vec<String>,
}

impl Display for VirtPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]", self.name)?;
        for part in self.locked_parts.iter() {
            write!(f, "/{}", part)?;
        }
        write!(f, "/$")?;
        for part in self.parts.iter() {
            write!(f, "/{}", part)?;
        }
        Ok(())
    }
}

impl Referrable for VirtPath {
    fn format_ref(
        &self,
        left: usize,
        _right: usize,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        let fs_path = self.to_path_buf();
        let code = fs::read_to_string(fs_path.clone()).unwrap();
        let before = code[..left].to_string();
        let lines = before.lines().count();
        let column = if let Some(last_line) = before.lines().last() {
            last_line.len() + 1
        } else {
            0
        };
        write!(f, "{}:{}:{}", fs_path.display(), lines, column)
    }
}

impl VirtPath {
    pub fn lock(self) -> VirtPath {
        let mut res = self;
        res.locked_parts.append(&mut res.parts);
        res
    }

    pub fn parent<F: Clone>(self) -> Result<VirtPath, F> {
        let mut out = self;
        let res = out.parts.pop();
        match res {
            Some(_) => Ok(out),
            None => Err(Error::new(
                ErrorType::Custom,
                format!("Parent of {} locked", out),
            )),
        }
    }

    pub fn step<F: Clone>(self, elem: impl ToString) -> Result<VirtPath, F> {
        let mut out: VirtPath = self;
        let elem = elem.to_string();
        match elem.as_str() {
            ".." => Ok(out.parent()?),
            "." => Ok(out),
            _ => {
                out.parts.push(elem);
                Ok(out)
            }
        }
    }

    pub fn apply<F: Clone>(&self, ext: &str) -> Result<VirtPath, F> {
        let mut out = self.clone();
        for part in ext.split("/") {
            if !part.is_empty() {
                let last = out.parts.last_mut().ok_or_else(|| {
                    Error::new(
                        ErrorType::Custom,
                        format!(
                            "can't add suffix to path without unlocked elements: {}",
                            self
                        ),
                    )
                })?;
                last.push_str(part);
            }
            out.parts.push("".into());
        }
        let _ = out.parts.pop();
        Ok(out)
    }

    pub fn add_suffix<F: Clone>(&self, suffix: &str) -> Result<VirtPath, F> {
        let mut out = self.clone();
        let last = out.parts.last_mut().ok_or_else(|| {
            Error::new(
                ErrorType::Custom,
                format!(
                    "can't add suffix to path without unlocked elements: {}",
                    self
                ),
            )
        })?;
        last.push_str(suffix);
        Ok(out)
    }

    pub fn remove_suffix<F: Clone>(&self, suffix: &str) -> Result<VirtPath, F> {
        let mut out = self.clone();
        let last = out.parts.pop().ok_or_else(|| {
            Error::new(
                ErrorType::Custom,
                format!(
                    "can't remove suffix from path without unlocked elements: {}",
                    out
                ),
            )
        })?;
        let last_prefix = last.strip_suffix(suffix).ok_or_else(|| {
            Error::new(
                ErrorType::Custom,
                format!("Invalid suffix {} for path {}", suffix, self),
            )
        })?;
        out.parts.push(last_prefix.to_string());
        Ok(out)
    }

    pub fn to_path_buf(&self) -> PathBuf {
        let mut cur_path = self.root.clone();
        for part in self.locked_parts.iter() {
            cur_path.push(part);
        }
        for part in self.parts.iter() {
            cur_path.push(part);
        }
        cur_path
    }

    pub fn to_path_buf_rebase<F: Clone>(&self, base: &VirtPath) -> Result<PathBuf, F> {
        let self_path = self.to_path_buf();
        let base_path = base.to_path_buf();
        if let Some(relative_path) = diff_paths(self_path, base_path) {
            Ok(relative_path)
        } else {
            Err(Error::new(
                ErrorType::Custom,
                format!("Failed to compute relative path from {} to {}", self, base),
            ))
        }
    }

    pub fn virtualize(path: &Path, name: impl ToString) -> VirtPath {
        VirtPath {
            name: name.to_string(),
            root: path.parent().unwrap().to_path_buf(),
            locked_parts: vec![],
            parts: vec![path.file_name().unwrap().to_str().unwrap().into()],
        }
    }

    pub fn translate(self, from: &VirtPath, to: &VirtPath) -> Option<VirtPath> {
        if self.name != from.name || self.root != from.root {
            None
        } else {
            let self_path = [self.locked_parts.clone(), self.parts.clone()].concat();
            let from_path = [from.locked_parts.clone(), from.parts.clone()].concat();

            if let Some(suffix) = self_path.strip_prefix(from_path.as_slice()) {
                let mut new_parts = to.parts.clone();
                let suffix = suffix.iter().cloned();
                new_parts.extend(suffix);
                Some(VirtPath {
                    name: to.name.clone(),
                    root: to.root.clone(),
                    locked_parts: to.locked_parts.clone(),
                    parts: new_parts,
                })
            } else {
                None
            }
        }
    }

    #[cfg(test)]
    pub fn new(name: impl ToString) -> VirtPath {
        VirtPath {
            name: name.to_string(),
            root: PathBuf::from("."),
            locked_parts: vec![],
            parts: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestF = i32;

    #[test]
    fn test_step_down() {
        let path = VirtPath::new("root");
        assert_eq!(path.to_string().as_str(), "[root]/$");
        let path = path.step::<TestF>("test").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test");
        let path = path.step::<TestF>("b").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test/b");
        let path = path.step::<TestF>("..").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test");
        let path = path.step::<TestF>(".").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test");
        let path = path.step::<TestF>("..").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$");
        assert!(path.step::<TestF>("..").is_err());
    }

    #[test]
    fn test_lock() {
        let path = VirtPath::new("root");
        assert_eq!(path.to_string().as_str(), "[root]/$");
        let path = path.step::<TestF>("test").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test");
        let path = path.lock();
        assert_eq!(path.to_string().as_str(), "[root]/test/$");
        assert!(path.step::<TestF>("..").is_err());
    }

    #[test]
    fn test_relock() {
        let path = VirtPath::new("root");
        assert_eq!(path.to_string().as_str(), "[root]/$");
        let path = path.step::<TestF>("test").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test");
        let path = path.lock();
        assert_eq!(path.to_string().as_str(), "[root]/test/$");
        let path = path.step::<TestF>("b").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/test/$/b");
        let path = path.lock();
        assert_eq!(path.to_string().as_str(), "[root]/test/b/$");
    }

    #[test]
    fn test_path_root_eq() {
        let path_a1 = VirtPath::new("a");
        let path_a2 = VirtPath::new("a");
        let path_b = VirtPath::new("b");
        assert_eq!(path_a1, path_a2);
        assert_ne!(path_a1, path_b);
        assert_ne!(path_a2, path_b);
    }

    #[test]
    fn test_path_path_eq() {
        let base = VirtPath::new("a");
        let cur = VirtPath::new("a");
        let cur = cur.step::<TestF>("test").unwrap();
        assert_ne!(base, cur);
        let cur = cur.step::<TestF>("..").unwrap();
        assert_eq!(base, cur);
    }

    #[test]
    fn test_virtpath_to_path_buf() {
        let virtpath_a = PathBuf::from("./test_a");
        let virtpath_b = PathBuf::from("./test_b");

        assert_eq!(
            VirtPath::virtualize(&virtpath_a, "a").to_path_buf(),
            PathBuf::from("./test_a")
        );

        assert_eq!(
            VirtPath::virtualize(&virtpath_a, "a")
                .step::<TestF>("hej")
                .unwrap()
                .to_path_buf(),
            PathBuf::from("./test_a/hej")
        );

        assert_eq!(
            VirtPath::virtualize(&virtpath_a, "a")
                .step::<TestF>("hej")
                .unwrap()
                .lock()
                .to_path_buf(),
            PathBuf::from("./test_a/hej")
        );

        assert_eq!(
            VirtPath::virtualize(&virtpath_b, "b")
                .step::<TestF>("hej")
                .unwrap()
                .parent::<TestF>()
                .unwrap()
                .to_path_buf(),
            PathBuf::from("./test_b")
        );
    }

    #[test]
    fn test_from_path() {
        let filepath = PathBuf::from("./test/file.txt");
        let virtpath = VirtPath::virtualize(&filepath, "root");

        assert_eq!(
            virtpath,
            VirtPath {
                name: "root".into(),
                root: PathBuf::from("./test"),
                locked_parts: vec![],
                parts: vec!["file.txt".into()]
            }
        );
    }

    #[test]
    fn test_translate() {
        let src_dir = VirtPath::virtualize(&PathBuf::from("./src"), "src");
        let build_dir = VirtPath::virtualize(&PathBuf::from("./build"), "build")
            .step::<TestF>("subproj")
            .unwrap();

        let src_file = src_dir
            .clone()
            .step::<TestF>("lib")
            .unwrap()
            .step::<TestF>("source.c")
            .unwrap();
        let exp_obj_file = build_dir
            .clone()
            .step::<TestF>("lib")
            .unwrap()
            .step::<TestF>("source.c")
            .unwrap();

        assert_eq!(src_file.translate(&src_dir, &build_dir), Some(exp_obj_file));
    }

    #[test]
    fn test_translate_invalid_root() {
        let src_dir = VirtPath::virtualize(&PathBuf::from("./src"), "src");
        let build_dir = VirtPath::virtualize(&PathBuf::from("./build"), "build")
            .step::<TestF>("subproj")
            .unwrap();

        let src_subdir = src_dir.clone().step::<TestF>("otherdir").unwrap();

        let src_file = src_dir
            .clone()
            .step::<TestF>("lib")
            .unwrap()
            .step::<TestF>("source.c")
            .unwrap();

        assert_eq!(src_file.translate(&src_subdir, &build_dir), None);
    }

    #[test]
    fn test_apply_parts() {
        assert_eq!(
            VirtPath::new("root")
                .step::<TestF>("test")
                .unwrap()
                .step::<TestF>("src.s")
                .unwrap(),
            VirtPath::new("root").apply::<i32>("/test/src.s").unwrap()
        )
    }

    #[test]
    fn test_apply_suffix() {
        assert_eq!(
            VirtPath::new("root").step::<TestF>("test.o").unwrap(),
            VirtPath::new("root")
                .step::<TestF>("test")
                .unwrap()
                .apply::<TestF>(".o")
                .unwrap()
        )
    }

    #[test]
    fn test_add_suffix() {
        assert_eq!(
            VirtPath::new("root").step::<TestF>("test.o").unwrap(),
            VirtPath::new("root")
                .step::<TestF>("test")
                .unwrap()
                .add_suffix::<TestF>(".o")
                .unwrap()
        )
    }

    #[test]
    fn test_add_suffix_only_last_element() {
        assert_eq!(
            VirtPath::new("root")
                .step::<TestF>("dir")
                .unwrap()
                .step::<TestF>("file.o")
                .unwrap(),
            VirtPath::new("root")
                .step::<TestF>("dir")
                .unwrap()
                .step::<TestF>("file")
                .unwrap()
                .add_suffix::<TestF>(".o")
                .unwrap()
        )
    }

    #[test]
    fn test_remove_suffix() {
        let base: VirtPath = VirtPath::new("root");
        assert_eq!(
            base.clone()
                .step::<TestF>("src.s")
                .unwrap()
                .remove_suffix::<TestF>(".s")
                .unwrap(),
            base.clone().step::<TestF>("src").unwrap()
        )
    }

    #[test]
    fn test_remove_suffix_fail() {
        let base: VirtPath = VirtPath::new("root");
        assert!(
            base.step::<TestF>("src.s")
                .unwrap()
                .remove_suffix::<TestF>(".c")
                .is_err()
        )
    }
}
