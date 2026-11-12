//! Suggestions for a name that was not found.

/// The number of single-character edits between two names.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substituted = diagonal + usize::from(ca != *cb);
            diagonal = row[j + 1];
            row[j + 1] = substituted.min(row[j] + 1).min(diagonal + 1);
        }
    }
    row[b.len()]
}

/// The candidate most likely meant by `name`, if any is close enough to be
/// a typing mistake: at most a third of the name's length in edits, and at
/// least one.
pub fn closest(name: &str, candidates: impl IntoIterator<Item = String>) -> Option<String> {
    let limit = (name.chars().count() / 3).max(1);
    candidates
        .into_iter()
        .filter(|candidate| candidate != name)
        .map(|candidate| (distance(name, &candidate), candidate))
        .filter(|(distance, _)| *distance <= limit)
        .min()
        .map(|(_, candidate)| candidate)
}

/// A list of names for a message: `` `a`, `b` and `c` ``.
pub fn name_list(names: impl IntoIterator<Item = String>) -> String {
    let names: Vec<String> = names.into_iter().map(|name| format!("`{name}`")).collect();
    match names.split_last() {
        None => String::new(),
        Some((only, [])) => only.clone(),
        Some((last, rest)) => format!("{} and {}", rest.join(", "), last),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn distance_counts_edits() {
        assert_eq!(distance("name", "name"), 0);
        assert_eq!(distance("nam", "name"), 1);
        assert_eq!(distance("modd", "mod"), 1);
        assert_eq!(distance("abc", "cba"), 2);
        assert_eq!(distance("", "abc"), 3);
    }

    #[test]
    fn closest_picks_a_likely_typo_only() {
        assert_eq!(
            closest("nam", names(&["cwd", "name", "lib"])),
            Some("name".to_string())
        );
        assert_eq!(
            closest("modd", names(&["mod", "mod_fn", "build"])),
            Some("mod".to_string())
        );
        // Nothing similar enough, and the name itself is no suggestion
        assert_eq!(closest("optlevel", names(&["src", "flags"])), None);
        assert_eq!(closest("src", names(&["src"])), None);
    }

    #[test]
    fn name_list_reads_as_a_sentence() {
        assert_eq!(name_list(names(&[])), "");
        assert_eq!(name_list(names(&["a"])), "`a`");
        assert_eq!(name_list(names(&["a", "b"])), "`a` and `b`");
        assert_eq!(name_list(names(&["a", "b", "c"])), "`a`, `b` and `c`");
    }
}
