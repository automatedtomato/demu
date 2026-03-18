// `ls` command — list directory contents.
//
// Lists the direct children of the given path in the virtual filesystem.
// Children are sorted alphabetically. In long format, a type prefix
// character (`d`, `-`, `l`) is prepended. Symlinks include their target.

use std::io::Write;
use std::path::Path;

use crate::model::fs::FsNode;
use crate::model::state::PreviewState;
use crate::repl::error::ReplError;
use crate::repl::path::resolve_path;

/// Execute the `ls` command.
///
/// Lists the direct children of `path` (or `state.cwd` when `path` is `None`)
/// sorted alphabetically. Directories get a trailing `/`.
///
/// In long format (`long = true`) each entry is prefixed with a type indicator:
/// - `d` for directories
/// - `-` for regular files
/// - `l` for symbolic links (shows `name -> target`)
pub fn execute(
    state: &PreviewState,
    path: Option<&str>,
    long: bool,
    writer: &mut impl Write,
) -> Result<(), ReplError> {
    // Resolve the target path: use explicit argument or fall back to cwd.
    let target = match path {
        Some(p) => resolve_path(&state.cwd, p),
        None => state.cwd.clone(),
    };

    // Verify the target path exists in the virtual filesystem.
    // The root `/` is always valid. For other paths, accept the path if:
    // 1. There is an explicit node at that path (e.g. a DirNode), OR
    // 2. There are children of that path (an implicit directory whose node
    //    was never explicitly inserted — a valid state in VirtualFs).
    if target != Path::new("/")
        && !state.fs.contains(&target)
        && state.fs.list_dir(&target).is_empty()
    {
        return Err(ReplError::PathNotFound { path: target });
    }

    // Collect direct children.
    let mut children = state.fs.list_dir(&target);

    // Sort alphabetically by the final path component (case-sensitive).
    children.sort_by(|(a, _), (b, _)| {
        let a_name = a.file_name().unwrap_or(a.as_os_str());
        let b_name = b.file_name().unwrap_or(b.as_os_str());
        a_name.cmp(b_name)
    });

    // Write each entry.
    for (child_path, node) in &children {
        let name = child_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");

        format_entry(name, node, long, writer).map_err(|e| ReplError::InvalidArguments {
            command: "ls".to_string(),
            message: e.to_string(),
        })?;
    }

    Ok(())
}

/// Write a single directory entry to the writer in short or long format.
fn format_entry(
    name: &str,
    node: &FsNode,
    long: bool,
    writer: &mut impl Write,
) -> std::io::Result<()> {
    match node {
        FsNode::Directory(_) => {
            if long {
                writeln!(writer, "drwxr-xr-x {name}/")
            } else {
                writeln!(writer, "{name}/")
            }
        }
        FsNode::File(_) => {
            if long {
                writeln!(writer, "-rw-r--r-- {name}")
            } else {
                writeln!(writer, "{name}")
            }
        }
        FsNode::Symlink(s) => {
            let target = s.target.display();
            if long {
                writeln!(writer, "lrwxrwxrwx {name} -> {target}")
            } else {
                writeln!(writer, "{name} -> {target}")
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::fs::{DirNode, FileNode, FsNode, SymlinkNode, VirtualFs};
    use crate::model::provenance::{Provenance, ProvenanceSource};
    use crate::model::state::PreviewState;
    use std::path::PathBuf;

    fn make_provenance() -> Provenance {
        Provenance::new(ProvenanceSource::Workdir)
    }

    fn file_node(content: &[u8]) -> FsNode {
        FsNode::File(FileNode {
            content: content.to_vec(),
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

    fn symlink_node(target: &str) -> FsNode {
        FsNode::Symlink(SymlinkNode {
            target: PathBuf::from(target),
            provenance: make_provenance(),
        })
    }

    fn run(state: &PreviewState, path: Option<&str>, long: bool) -> String {
        let mut buf = Vec::new();
        execute(state, path, long, &mut buf).expect("ls should not fail in this test");
        String::from_utf8(buf).expect("utf-8")
    }

    fn run_result(
        state: &PreviewState,
        path: Option<&str>,
        long: bool,
    ) -> Result<String, ReplError> {
        let mut buf = Vec::new();
        execute(state, path, long, &mut buf)?;
        Ok(String::from_utf8(buf).expect("utf-8"))
    }

    fn state_with_fs(fs: VirtualFs) -> PreviewState {
        let mut state = PreviewState::default();
        state.fs = fs;
        state
    }

    // --- Empty directory ---

    #[test]
    fn ls_empty_fs_at_root_prints_nothing() {
        let state = PreviewState::default();
        let out = run(&state, None, false);
        assert_eq!(out, "");
    }

    // --- Default to cwd ---

    #[test]
    fn ls_no_path_uses_cwd() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/main.rs"), file_node(b""));
        let mut state = state_with_fs(fs);
        state.cwd = PathBuf::from("/app");
        let out = run(&state, None, false);
        assert!(out.contains("main.rs"), "got: {out}");
    }

    // --- Sorted alphabetically ---

    #[test]
    fn ls_entries_sorted_alphabetically() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/zebra.txt"), file_node(b""));
        fs.insert(PathBuf::from("/app/alpha.txt"), file_node(b""));
        fs.insert(PathBuf::from("/app/middle.txt"), file_node(b""));
        let state = state_with_fs(fs);
        let out = run(&state, Some("/app"), false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines, vec!["alpha.txt", "middle.txt", "zebra.txt"]);
    }

    // --- Trailing slash on directories ---

    #[test]
    fn ls_directories_have_trailing_slash() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        fs.insert(PathBuf::from("/etc"), dir_node());
        let state = state_with_fs(fs);
        let out = run(&state, Some("/"), false);
        assert!(
            out.contains("app/"),
            "directories must have trailing slash, got: {out}"
        );
        assert!(
            out.contains("etc/"),
            "directories must have trailing slash, got: {out}"
        );
    }

    #[test]
    fn ls_files_have_no_trailing_slash() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/file.txt"), file_node(b""));
        let state = state_with_fs(fs);
        let out = run(&state, Some("/app"), false);
        assert!(out.contains("file.txt"), "got: {out}");
        assert!(
            !out.contains("file.txt/"),
            "files must not have trailing slash, got: {out}"
        );
    }

    // --- Long format ---

    #[test]
    fn ls_long_format_dir_starts_with_d() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        let state = state_with_fs(fs);
        let out = run(&state, Some("/"), true);
        assert!(
            out.starts_with('d'),
            "long format dir must start with 'd', got: {out}"
        );
    }

    #[test]
    fn ls_long_format_file_starts_with_dash() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/file.txt"), file_node(b""));
        let state = state_with_fs(fs);
        let out = run(&state, Some("/app"), true);
        assert!(
            out.starts_with('-'),
            "long format file must start with '-', got: {out}"
        );
    }

    #[test]
    fn ls_long_format_symlink_starts_with_l_and_shows_target() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/link"), symlink_node("/usr/bin/python3"));
        let state = state_with_fs(fs);
        let out = run(&state, Some("/app"), true);
        assert!(
            out.starts_with('l'),
            "long format symlink must start with 'l', got: {out}"
        );
        assert!(
            out.contains("-> /usr/bin/python3"),
            "must show target, got: {out}"
        );
    }

    // --- Symlink in short format ---

    #[test]
    fn ls_short_format_symlink_shows_arrow_and_target() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/link"), symlink_node("/usr/lib/lib.so"));
        let state = state_with_fs(fs);
        let out = run(&state, Some("/app"), false);
        assert!(out.contains("link -> /usr/lib/lib.so"), "got: {out}");
    }

    // --- Nonexistent path ---

    #[test]
    fn ls_nonexistent_path_returns_path_not_found() {
        let state = PreviewState::default();
        let result = run_result(&state, Some("/nonexistent"), false);
        assert!(matches!(result, Err(ReplError::PathNotFound { .. })));
    }

    // --- Only direct children, not grandchildren ---

    #[test]
    fn ls_does_not_list_grandchildren() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        fs.insert(PathBuf::from("/app/src"), dir_node());
        // Grandchild — must NOT appear in ls("/")
        fs.insert(PathBuf::from("/app/src/main.rs"), file_node(b""));
        let state = state_with_fs(fs);
        let out = run(&state, Some("/"), false);
        assert!(
            !out.contains("main.rs"),
            "grandchildren must not appear, got: {out}"
        );
        assert!(
            !out.contains("src"),
            "grandchildren must not appear, got: {out}"
        );
        assert!(
            out.contains("app/"),
            "direct child app must appear, got: {out}"
        );
    }

    // --- Mixed content ---

    #[test]
    fn ls_mixed_files_and_dirs_sorted() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/root/b_dir"), dir_node());
        fs.insert(PathBuf::from("/root/a_file.txt"), file_node(b""));
        let state = state_with_fs(fs);
        let out = run(&state, Some("/root"), false);
        let lines: Vec<&str> = out.lines().collect();
        // a_file.txt < b_dir alphabetically
        assert_eq!(lines[0], "a_file.txt");
        assert_eq!(lines[1], "b_dir/");
    }
}
