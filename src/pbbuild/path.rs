use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

use pathdiff::diff_paths;

use crate::pbexpr::{Error, ErrorType, Referrable, Result, SourceContext};

/// An empty relative path means the current directory, which is written `.`.
fn non_empty_path(path: PathBuf) -> PathBuf {
    if path.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        path
    }
}

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
        start: usize,
        _end: usize,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        let fs_path = self.to_path_buf();
        // Without the text there is no line to count to, so the byte offset
        // has to do
        let Ok(code) = fs::read_to_string(&fs_path) else {
            return write!(f, "{}:@{}", fs_path.display(), start);
        };
        let before = &code[..start.min(code.len())];
        let line = before.matches('\n').count() + 1;
        let column = before.len() - before.rfind('\n').map_or(0, |pos| pos + 1) + 1;
        write!(f, "{}:{}:{}", fs_path.display(), line, column)
    }
}

impl SourceContext for VirtPath {
    fn source_text(&self) -> Option<String> {
        fs::read_to_string(self.to_path_buf()).ok()
    }

    fn display_path(&self) -> String {
        self.to_path_buf().display().to_string()
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
                format!("\"..\" would leave the locked directory {}", out),
            )),
        }
    }

    /// Add one element, given as text, below the path. `..` steps up within
    /// the unlocked part, `.` and empty elements are dropped. This is the only
    /// way text may become a new element, so the lock can't be escaped.
    fn push_elem<F: Clone>(self, elem: &str) -> Result<VirtPath, F> {
        let mut out = self;
        match elem {
            ".." => out.parent(),
            "." | "" => Ok(out),
            _ => {
                out.parts.push(elem.to_string());
                Ok(out)
            }
        }
    }

    /// Check that editing the last element left a real name, not something
    /// that would be resolved as a directory reference.
    fn check_last<F: Clone>(self, edit: &str) -> Result<VirtPath, F> {
        match self.parts.last().map(String::as_str) {
            Some("") | Some(".") | Some("..") => Err(Error::new(
                ErrorType::Custom,
                format!("{} makes an invalid path element: {}", edit, self),
            )),
            _ => Ok(self),
        }
    }

    fn check_suffix<F: Clone>(&self, suffix: &str) -> Result<(), F> {
        if suffix.contains('/') {
            Err(Error::new(
                ErrorType::Custom,
                format!(
                    "suffix \"{}\" of {} contains \"/\"; use / or \"${{path}}/...\" to add elements",
                    suffix, self
                ),
            ))
        } else {
            Ok(())
        }
    }

    /// Add elements below the path. `elem` may contain `/`, but can't be
    /// absolute.
    pub fn step<F: Clone>(self, elem: impl ToString) -> Result<VirtPath, F> {
        let elem = elem.to_string();
        if elem.starts_with('/') {
            return Err(Error::new(
                ErrorType::Custom,
                format!("can't add absolute path \"{}\" to {}", elem, self),
            ));
        }
        elem.split('/').try_fold(self, |out, part| out.push_elem(part))
    }

    /// Apply the text that follows a path in a string, as in `"${path}.c"`
    /// or `"${path}/sub/file.c"`. Text before the first `/` is a suffix of
    /// the last element, each piece after a `/` is a new element.
    pub fn apply<F: Clone>(&self, ext: &str) -> Result<VirtPath, F> {
        let mut pieces = ext.split('/');
        let suffix = pieces.next().unwrap_or_default();
        let out = if suffix.is_empty() {
            self.clone()
        } else {
            self.add_suffix(suffix)?
        };
        pieces.try_fold(out, |out, part| out.push_elem(part))
    }

    pub fn add_suffix<F: Clone>(&self, suffix: &str) -> Result<VirtPath, F> {
        self.check_suffix(suffix)?;
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
        out.check_last(&format!("suffix \"{}\"", suffix))
    }

    pub fn remove_suffix<F: Clone>(&self, suffix: &str) -> Result<VirtPath, F> {
        self.check_suffix(suffix)?;
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
        out.check_last(&format!("removing suffix \"{}\"", suffix))
    }

    /// The joined path, which is empty for the directory of a relative root
    /// such as the one of `main.pbb`.
    fn join_parts(&self) -> PathBuf {
        let mut cur_path = self.root.clone();
        for part in self.locked_parts.iter() {
            cur_path.push(part);
        }
        for part in self.parts.iter() {
            cur_path.push(part);
        }
        cur_path
    }

    /// The path in the file system. Never empty: the current directory is
    /// `.`.
    pub fn to_path_buf(&self) -> PathBuf {
        non_empty_path(self.join_parts())
    }

    /// The unlocked part of the path, with `/` as separator. Independent of
    /// where the build is located in the file system.
    pub fn to_unlocked_string(&self) -> String {
        self.parts.join("/")
    }

    /// The path relative to `base`. Never empty: the same path is `.`.
    pub fn to_path_buf_rebase<F: Clone>(&self, base: &VirtPath) -> Result<PathBuf, F> {
        // Not `to_path_buf()`: a leading `.` would make `diff_paths` add a
        // `.` element to the result.
        let self_path = self.join_parts();
        let base_path = base.join_parts();
        if let Some(relative_path) = diff_paths(self_path, base_path) {
            Ok(non_empty_path(relative_path))
        } else {
            Err(Error::new(
                ErrorType::Custom,
                format!("Failed to compute relative path from {} to {}", self, base),
            ))
        }
    }

    pub fn from_dir(path: &Path, name: impl ToString) -> VirtPath {
        VirtPath {
            name: name.to_string(),
            root: path.to_path_buf(),
            locked_parts: vec![],
            parts: vec![],
        }
    }

    pub fn from_file(path: &Path, name: impl ToString) -> VirtPath {
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
            VirtPath::from_file(&virtpath_a, "a").to_path_buf(),
            PathBuf::from("./test_a")
        );

        assert_eq!(
            VirtPath::from_file(&virtpath_a, "a")
                .step::<TestF>("hej")
                .unwrap()
                .to_path_buf(),
            PathBuf::from("./test_a/hej")
        );

        assert_eq!(
            VirtPath::from_file(&virtpath_a, "a")
                .step::<TestF>("hej")
                .unwrap()
                .lock()
                .to_path_buf(),
            PathBuf::from("./test_a/hej")
        );

        assert_eq!(
            VirtPath::from_file(&virtpath_b, "b")
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
        let virtpath = VirtPath::from_file(&filepath, "root");

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
        let src_dir = VirtPath::from_file(&PathBuf::from("./src"), "src");
        let build_dir = VirtPath::from_file(&PathBuf::from("./build"), "build")
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
        let src_dir = VirtPath::from_file(&PathBuf::from("./src"), "src");
        let build_dir = VirtPath::from_file(&PathBuf::from("./build"), "build")
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

    fn locked_cwd() -> VirtPath {
        VirtPath::new("root").step::<TestF>("dir").unwrap().lock()
    }

    #[test]
    fn test_apply_parent_of_locked() {
        assert!(locked_cwd().apply::<TestF>("/../x").is_err());
        assert!(locked_cwd().apply::<TestF>("/a/../../x").is_err());
    }

    #[test]
    fn test_apply_parent_below_lock() {
        assert_eq!(
            locked_cwd().apply::<TestF>("/a/../b").unwrap().to_string(),
            "[root]/dir/$/b"
        );
    }

    #[test]
    fn test_apply_dot() {
        assert_eq!(
            locked_cwd().apply::<TestF>("/./x").unwrap().to_string(),
            "[root]/dir/$/x"
        );
    }

    #[test]
    fn test_apply_empty_elements() {
        assert_eq!(
            locked_cwd().apply::<TestF>("//x/").unwrap().to_string(),
            "[root]/dir/$/x"
        );
    }

    #[test]
    fn test_step_with_slash() {
        assert!(locked_cwd().step::<TestF>("../x").is_err());
        assert_eq!(
            locked_cwd().step::<TestF>("a/../b").unwrap().to_string(),
            "[root]/dir/$/b"
        );
    }

    #[test]
    fn test_step_absolute() {
        assert!(locked_cwd().step::<TestF>("/etc").is_err());
    }

    #[test]
    fn test_suffix_with_slash() {
        let file = locked_cwd().step::<TestF>("file").unwrap();
        assert!(file.add_suffix::<TestF>("/../../x").is_err());
        assert!(file.add_suffix::<TestF>("/x").is_err());
        let file = locked_cwd().step::<TestF>("a/b.c").unwrap();
        assert!(file.remove_suffix::<TestF>("a/b.c").is_err());
        assert!(file.remove_suffix::<TestF>("/b.c").is_err());
    }

    #[test]
    fn test_remove_suffix_empty_name() {
        let file = locked_cwd().step::<TestF>("a.c").unwrap();
        assert!(file.remove_suffix::<TestF>("a.c").is_err());
        let file = locked_cwd().step::<TestF>(".c").unwrap();
        assert!(file.remove_suffix::<TestF>(".c").is_err());
    }

    #[test]
    fn test_suffix_makes_parent() {
        let empty = locked_cwd().step::<TestF>("").unwrap();
        assert!(empty.apply::<TestF>("..").is_err());
        let file = locked_cwd().step::<TestF>("..b").unwrap();
        assert!(file.remove_suffix::<TestF>("b").is_err());
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

    /// `pb` reads `main.pbb` by default, whose parent is the empty path.
    fn bare_file_cwd() -> VirtPath {
        VirtPath::from_file(&PathBuf::from("main.pbb"), "root")
            .parent::<TestF>()
            .unwrap()
            .lock()
    }

    #[test]
    fn test_current_dir_is_dot() {
        let cwd = bare_file_cwd();
        assert_eq!(cwd.to_path_buf(), PathBuf::from("."));
        assert_eq!(
            cwd.step::<TestF>("a/..").unwrap().to_path_buf(),
            PathBuf::from(".")
        );
    }

    #[test]
    fn test_below_current_dir_has_no_dot() {
        let file = bare_file_cwd().step::<TestF>("myfile.txt").unwrap();
        assert_eq!(file.to_path_buf(), PathBuf::from("myfile.txt"));
    }

    #[test]
    fn test_rebase_same_is_dot() {
        let cwd = bare_file_cwd();
        let sub = cwd.clone().step::<TestF>("sub").unwrap();
        let rebase = |path: &VirtPath, base: &VirtPath| {
            path.to_path_buf_rebase::<TestF>(base).unwrap()
        };
        assert_eq!(rebase(&cwd, &cwd), PathBuf::from("."));
        assert_eq!(rebase(&sub, &sub), PathBuf::from("."));
        assert_eq!(rebase(&cwd, &sub), PathBuf::from(".."));
        assert_eq!(rebase(&sub, &cwd), PathBuf::from("sub"));
    }
}
