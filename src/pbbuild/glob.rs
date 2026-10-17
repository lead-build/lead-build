//! Glob-style pattern matching, used by `pb.matches`.
//!
//! Syntax is that of `globset`, with `*` and `?` not matching `/`, and `**`
//! matching across directories. A pattern without `/` is matched against the
//! last component of the subject only, so `*.c` matches `src/main.c`.

use globset::{GlobBuilder, GlobMatcher};

#[derive(Debug)]
pub struct Glob {
    matcher: GlobMatcher,
    basename_only: bool,
}

impl Glob {
    pub fn new(pattern: &str) -> Result<Glob, globset::Error> {
        let matcher = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()?
            .compile_matcher();
        Ok(Glob {
            matcher,
            basename_only: !pattern.contains('/'),
        })
    }

    pub fn is_match(&self, subject: &str) -> bool {
        let subject = if self.basename_only {
            subject.rsplit('/').next().unwrap_or(subject)
        } else {
            subject
        };
        self.matcher.is_match(subject)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glob_match(pattern: &str, subject: &str) -> bool {
        Glob::new(pattern).unwrap().is_match(subject)
    }

    #[test]
    fn literal() {
        assert!(glob_match("main.c", "main.c"));
        assert!(!glob_match("main.c", "main.h"));
        assert!(!glob_match("main.c", "main.cc"));
    }

    #[test]
    fn star_does_not_cross_directories() {
        assert!(glob_match("src/*.c", "src/main.c"));
        assert!(!glob_match("src/*.c", "src/sub/main.c"));
    }

    #[test]
    fn basename_only_without_slash() {
        assert!(glob_match("*.c", "/abs/src/main.c"));
        assert!(glob_match("main.?", "src/main.c"));
        assert!(!glob_match("src", "src/main.c"));
    }

    #[test]
    fn double_star() {
        assert!(glob_match("**/main.c", "/abs/src/main.c"));
        assert!(glob_match("**/src/**/*.c", "/abs/src/a/b/main.c"));
        assert!(!glob_match("**/src/**/*.c", "/abs/lib/a/b/main.c"));
    }

    #[test]
    fn classes_and_alternatives() {
        assert!(glob_match("*.[ch]", "main.h"));
        assert!(!glob_match("*.[ch]", "main.o"));
        assert!(glob_match("*.{c,cpp}", "main.cpp"));
    }

    #[test]
    fn invalid_pattern() {
        assert!(Glob::new("[").is_err());
    }
}
