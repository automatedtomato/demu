// `find` command — search the virtual filesystem.
//
// Lists all filesystem entries under the given root path, optionally
// filtered by a simple glob pattern matching the final filename component.
// Glob matching supports `*` (any sequence of chars) and `?` (one char).

use std::io::Write;
use std::path::Path;

use crate::model::state::PreviewState;
use crate::repl::error::ReplError;
use crate::repl::path::resolve_path;

/// Execute the `find` command.
///
/// Searches the virtual filesystem for all entries whose path starts with
/// the resolved `path` prefix. If `name_pattern` is provided, only entries
/// whose final filename component matches the glob are included.
///
/// Entries are printed in sorted order (one per line) to produce deterministic
/// output regardless of the internal `HashMap` iteration order.
pub fn execute(
    state: &PreviewState,
    path: &str,
    name_pattern: Option<&str>,
    writer: &mut impl Write,
) -> Result<(), ReplError> {
    let base = resolve_path(&state.cwd, path);

    // Validate that the base path exists (or is root, which is implicit).
    // Accept the path if it has an explicit node OR if it has children
    // (an implicit directory whose node was not explicitly inserted).
    let base_exists = base == Path::new("/")
        || state.fs.contains(&base)
        || state
            .fs
            .iter()
            .any(|(p, _)| p.starts_with(&base) && p != &base);

    if !base_exists {
        return Err(ReplError::PathNotFound { path: base });
    }

    // Collect all paths that are descendants of (or equal to) the base.
    let mut matches: Vec<String> = state
        .fs
        .iter()
        .filter(|(p, _)| is_descendant_or_equal(&base, p))
        .filter(|(p, _)| {
            // If a name pattern is given, match only against the final component.
            if let Some(pattern) = name_pattern {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| glob_match(pattern, n))
                    .unwrap_or(false)
            } else {
                true
            }
        })
        .map(|(p, _)| p.display().to_string())
        .collect();

    // Sort for deterministic output.
    matches.sort();

    for m in &matches {
        writeln!(writer, "{m}").map_err(|e| ReplError::Io {
            command: "find".to_string(),
            message: e.to_string(),
        })?;
    }

    Ok(())
}

/// Return `true` when `candidate` is a descendant of or equal to `base`.
///
/// Uses component-level prefix comparison so `/appdata/x` does not match
/// the base `/app`.
fn is_descendant_or_equal(base: &Path, candidate: &Path) -> bool {
    candidate.starts_with(base)
}

/// Match `name` against a simple glob `pattern` (only `*` and `?` supported).
///
/// - `*` matches any sequence of characters (including empty).
/// - `?` matches exactly one character.
/// - All other characters match literally.
///
/// Matching is case-sensitive and purely textual (no locale awareness).
fn glob_match(pattern: &str, name: &str) -> bool {
    glob_match_inner(pattern.as_bytes(), name.as_bytes())
}

/// Iterative DP glob match over byte slices.
///
/// Uses a standard dynamic-programming approach to avoid the exponential
/// worst-case of backtracking recursion. `dp[i][j]` is true when the first
/// `i` bytes of `pattern` match the first `j` bytes of `name`.
///
/// - `*` matches any sequence (including empty).
/// - `?` matches exactly one character.
/// - All other bytes match literally.
///
/// Using byte slices is safe here: `*` (0x2A) and `?` (0x3F) are ASCII and
/// will never appear as continuation bytes in multi-byte UTF-8 sequences.
fn glob_match_inner(pattern: &[u8], name: &[u8]) -> bool {
    let m = pattern.len();
    let n = name.len();

    // dp[i][j] = pattern[..i] matches name[..j]
    let mut dp = vec![vec![false; n + 1]; m + 1];
    dp[0][0] = true;

    // A leading sequence of `*` still matches an empty name.
    for i in 1..=m {
        if pattern[i - 1] == b'*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=m {
        for j in 1..=n {
            if pattern[i - 1] == b'*' {
                // Star matches empty (dp[i-1][j]) or one-more char (dp[i][j-1]).
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if pattern[i - 1] == b'?' || pattern[i - 1] == name[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }

    dp[m][n]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::fs::{DirNode, FileNode, FsNode, VirtualFs};
    use crate::model::provenance::{Provenance, ProvenanceSource};
    use crate::model::state::PreviewState;
    use std::path::PathBuf;

    fn make_provenance() -> Provenance {
        Provenance::new(ProvenanceSource::Workdir)
    }

    fn file_node() -> FsNode {
        FsNode::File(FileNode {
            content: vec![],
            provenance: make_provenance(),
            permissions: None,
        })
    }

    fn dir_node() -> FsNode {
        FsNode::Directory(DirNode {
            provenance: make_provenance(),
            permissions: None,
        })
    }

    fn state_with_fs(fs: VirtualFs) -> PreviewState {
        let mut state = PreviewState::default();
        state.fs = fs;
        state
    }

    fn run(
        state: &PreviewState,
        path: &str,
        pattern: Option<&str>,
    ) -> Result<Vec<String>, ReplError> {
        let mut buf = Vec::new();
        execute(state, path, pattern, &mut buf)?;
        let s = String::from_utf8(buf).expect("utf-8");
        Ok(s.lines().map(str::to_string).collect())
    }

    // --- No pattern: list all descendants ---

    #[test]
    fn find_no_pattern_lists_all_under_base() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        fs.insert(PathBuf::from("/app/main.rs"), file_node());
        fs.insert(PathBuf::from("/app/lib.rs"), file_node());
        let state = state_with_fs(fs);
        let lines = run(&state, "/app", None).expect("should succeed");
        // All three paths should appear (the dir itself + two files).
        assert!(lines.contains(&"/app".to_string()), "got: {lines:?}");
        assert!(
            lines.contains(&"/app/main.rs".to_string()),
            "got: {lines:?}"
        );
        assert!(lines.contains(&"/app/lib.rs".to_string()), "got: {lines:?}");
    }

    #[test]
    fn find_sorted_output() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/z.txt"), file_node());
        fs.insert(PathBuf::from("/app/a.txt"), file_node());
        fs.insert(PathBuf::from("/app/m.txt"), file_node());
        let state = state_with_fs(fs);
        let lines = run(&state, "/app", None).expect("should succeed");
        assert_eq!(lines, vec!["/app/a.txt", "/app/m.txt", "/app/z.txt"]);
    }

    // --- Pattern filtering ---

    #[test]
    fn find_name_star_rs_matches_rust_files() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/main.rs"), file_node());
        fs.insert(PathBuf::from("/app/lib.rs"), file_node());
        fs.insert(PathBuf::from("/app/Cargo.toml"), file_node());
        let state = state_with_fs(fs);
        let lines = run(&state, "/app", Some("*.rs")).expect("should succeed");
        assert!(
            lines.contains(&"/app/main.rs".to_string()),
            "got: {lines:?}"
        );
        assert!(lines.contains(&"/app/lib.rs".to_string()), "got: {lines:?}");
        assert!(
            !lines.contains(&"/app/Cargo.toml".to_string()),
            "got: {lines:?}"
        );
    }

    #[test]
    fn find_name_star_txt_matches_txt_files() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/readme.txt"), file_node());
        fs.insert(PathBuf::from("/app/notes.txt"), file_node());
        fs.insert(PathBuf::from("/app/main.rs"), file_node());
        let state = state_with_fs(fs);
        let lines = run(&state, "/app", Some("*.txt")).expect("should succeed");
        assert_eq!(lines.len(), 2, "got: {lines:?}");
    }

    #[test]
    fn find_name_question_mark_matches_single_char() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/a.txt"), file_node());
        fs.insert(PathBuf::from("/app/b.txt"), file_node());
        fs.insert(PathBuf::from("/app/ab.txt"), file_node());
        let state = state_with_fs(fs);
        let lines = run(&state, "/app", Some("?.txt")).expect("should succeed");
        // a.txt and b.txt match; ab.txt does not (two chars before .txt).
        assert!(lines.contains(&"/app/a.txt".to_string()), "got: {lines:?}");
        assert!(lines.contains(&"/app/b.txt".to_string()), "got: {lines:?}");
        assert!(
            !lines.contains(&"/app/ab.txt".to_string()),
            "got: {lines:?}"
        );
    }

    // --- Root search on empty filesystem ---

    #[test]
    fn find_root_on_empty_fs_returns_ok_with_empty_output() {
        let state = PreviewState::default(); // empty VirtualFs
        let lines = run(&state, "/", None).expect("find / on empty fs should return Ok");
        assert!(
            lines.is_empty(),
            "empty fs must produce no output, got: {lines:?}"
        );
    }

    // --- Pattern that matches nothing returns Ok with empty output ---

    #[test]
    fn find_unmatched_pattern_returns_ok_empty_output() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/main.rs"), file_node());
        fs.insert(PathBuf::from("/app/lib.rs"), file_node());
        let state = state_with_fs(fs);
        // *.txt matches nothing in a directory of .rs files.
        let lines = run(&state, "/app", Some("*.txt")).expect("unmatched pattern should return Ok");
        assert!(
            lines.is_empty(),
            "unmatched pattern must produce empty output, got: {lines:?}"
        );
    }

    // --- Nonexistent base ---

    #[test]
    fn find_nonexistent_base_returns_path_not_found() {
        let state = PreviewState::default();
        let result = run(&state, "/nonexistent", None);
        assert!(matches!(result, Err(ReplError::PathNotFound { .. })));
    }

    // --- Does not cross into sibling directories ---

    #[test]
    fn find_does_not_include_paths_outside_base() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/main.rs"), file_node());
        fs.insert(PathBuf::from("/etc/hosts"), file_node());
        let state = state_with_fs(fs);
        let lines = run(&state, "/app", None).expect("should succeed");
        assert!(!lines.contains(&"/etc/hosts".to_string()), "got: {lines:?}");
    }

    // --- Glob unit tests ---

    #[test]
    fn glob_star_matches_any_sequence() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("*.rs", "lib.rs"));
        assert!(!glob_match("*.rs", "Cargo.toml"));
    }

    #[test]
    fn glob_star_matches_empty() {
        // *.txt matches .txt (empty prefix)
        assert!(glob_match("*.txt", ".txt"));
    }

    #[test]
    fn glob_question_matches_single_char() {
        assert!(glob_match("?.txt", "a.txt"));
        assert!(!glob_match("?.txt", "ab.txt"));
    }

    #[test]
    fn glob_literal_match() {
        assert!(glob_match("main.rs", "main.rs"));
        assert!(!glob_match("main.rs", "lib.rs"));
    }

    #[test]
    fn glob_double_star_acts_as_multi_wildcard() {
        // Two consecutive stars still work (each * matches empty or more).
        assert!(glob_match("**/*.rs", "src/main.rs"));
    }
}
