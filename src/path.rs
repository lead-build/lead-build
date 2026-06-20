use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

use crate::lang::Referrable;

#[derive(Clone, PartialEq, Debug)]
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
        let lines = before.lines().into_iter().count();
        let column = before.lines().last().unwrap().len() + 1;
        write!(f, "{}:{}:{}", fs_path.display(), lines, column)
    }
}

impl VirtPath {
    pub fn lock(self) -> VirtPath {
        let mut res = self;
        res.locked_parts.append(&mut res.parts);
        res
    }

    pub fn parent(self) -> Option<VirtPath> {
        let mut out = self;
        out.parts.pop().map(|_| out)
    }

    pub fn step(self, elem: impl ToString) -> Option<VirtPath> {
        let mut out: VirtPath = self;
        let elem = elem.to_string();
        match elem.as_str() {
            ".." => out.parent(),
            "." => Some(out),
            _ => {
                out.parts.push(elem);
                Some(out)
            }
        }
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
                let suffix = suffix.iter().map(|s| s.clone());
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

    pub fn retype(self, from: &str, to: &str) -> Option<VirtPath> {
        let mut out = self;
        let last = out.parts.pop()?;
        let last_prefix = last.strip_suffix(from)?;
        out.parts.push(last_prefix.to_string() + to);
        Some(out)
    }

    #[cfg(test)]
    fn new(name: impl ToString) -> VirtPath {
        Self::virtualize(&PathBuf::from("/file"), name)
            .parent()
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_down() {
        let path = VirtPath::new("root");
        assert_eq!(path.to_string().as_str(), "[root]/$");
        let path = path.step("test").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test");
        let path = path.step("b").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test/b");
        let path = path.step("..").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test");
        let path = path.step(".").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test");
        let path = path.step("..").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$");
        assert!(path.step("..").is_none());
    }

    #[test]
    fn test_lock() {
        let path = VirtPath::new("root");
        assert_eq!(path.to_string().as_str(), "[root]/$");
        let path = path.step("test").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test");
        let path = path.lock();
        assert_eq!(path.to_string().as_str(), "[root]/test/$");
        assert!(path.step("..").is_none());
    }

    #[test]
    fn test_relock() {
        let path = VirtPath::new("root");
        assert_eq!(path.to_string().as_str(), "[root]/$");
        let path = path.step("test").unwrap();
        assert_eq!(path.to_string().as_str(), "[root]/$/test");
        let path = path.lock();
        assert_eq!(path.to_string().as_str(), "[root]/test/$");
        let path = path.step("b").unwrap();
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
        let cur = cur.step("test").unwrap();
        assert_ne!(base, cur);
        let cur = cur.step("..").unwrap();
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
                .step("hej")
                .unwrap()
                .to_path_buf(),
            PathBuf::from("./test_a/hej")
        );

        assert_eq!(
            VirtPath::virtualize(&virtpath_a, "a")
                .step("hej")
                .unwrap()
                .lock()
                .to_path_buf(),
            PathBuf::from("./test_a/hej")
        );

        assert_eq!(
            VirtPath::virtualize(&virtpath_b, "b")
                .step("hej")
                .unwrap()
                .parent()
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
            .step("subproj")
            .unwrap();

        let src_file = src_dir
            .clone()
            .step("lib")
            .unwrap()
            .step("source.c")
            .unwrap();
        let exp_obj_file = build_dir
            .clone()
            .step("lib")
            .unwrap()
            .step("source.c")
            .unwrap();

        assert_eq!(src_file.translate(&src_dir, &build_dir), Some(exp_obj_file));
    }

    #[test]
    fn test_translate_invalid_root() {
        let src_dir = VirtPath::virtualize(&PathBuf::from("./src"), "src");
        let build_dir = VirtPath::virtualize(&PathBuf::from("./build"), "build")
            .step("subproj")
            .unwrap();

        let src_subdir = src_dir.clone().step("otherdir").unwrap();

        let src_file = src_dir
            .clone()
            .step("lib")
            .unwrap()
            .step("source.c")
            .unwrap();

        assert_eq!(src_file.translate(&src_subdir, &build_dir), None);
    }

    #[test]
    fn test_retype() {
        assert_eq!(
            VirtPath::new("root")
                .step("test")
                .unwrap()
                .step("src.c")
                .unwrap()
                .retype(".c", ".o"),
            Some(
                VirtPath::new("root")
                    .step("test")
                    .unwrap()
                    .step("src.o")
                    .unwrap()
            )
        );
    }

    #[test]
    fn test_retype_invalid() {
        assert_eq!(
            VirtPath::new("root")
                .step("test")
                .unwrap()
                .step("src.s")
                .unwrap()
                .retype(".c", ".o"),
            None
        );
    }
}
