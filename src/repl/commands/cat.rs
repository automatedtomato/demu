// `cat` command — print file contents.
//
// Reads a file from the virtual filesystem and writes its content to the
// output writer. Symlinks print their target path. Directories produce an
// error. Missing paths produce a PathNotFound error.

use std::io::Write;

use crate::model::fs::FsNode;
use crate::model::state::PreviewState;
use crate::repl::error::ReplError;
use crate::repl::path::resolve_path;

/// Execute the `cat` command.
///
/// Resolves `path` relative to `state.cwd`, then:
/// - `FsNode::File` — writes content (using lossy UTF-8 for non-text bytes).
/// - `FsNode::Directory` — returns `ReplError::NotAFile`.
/// - `FsNode::Symlink` — writes a note showing the symlink target.
/// - Missing path — returns `ReplError::PathNotFound`.
pub fn execute(state: &PreviewState, path: &str, writer: &mut impl Write) -> Result<(), ReplError> {
    // Guard: empty path means the user typed `cat` with no argument.
    // Resolving an empty string produces the cwd (a directory), which would
    // give a confusing "is a directory" error instead of a usage hint.
    if path.is_empty() {
        return Err(ReplError::InvalidArguments {
            command: "cat".to_string(),
            message: "missing file path".to_string(),
        });
    }

    let resolved = resolve_path(&state.cwd, path);

    match state.fs.get(&resolved) {
        Some(FsNode::File(f)) => {
            // Use lossy conversion so non-UTF-8 bytes are displayed as U+FFFD
            // rather than causing a panic or hard error.
            let content = String::from_utf8_lossy(&f.content);
            write!(writer, "{content}").map_err(|e| ReplError::InvalidArguments {
                command: "cat".to_string(),
                message: e.to_string(),
            })
        }
        Some(FsNode::Directory(_)) => Err(ReplError::NotAFile { path: resolved }),
        Some(FsNode::Symlink(s)) => {
            // We don't follow symlinks automatically. Warn if the raw target
            // contains `..` components — any relative traversal from within a
            // container subtree could be misleading regardless of where it
            // ultimately resolves.
            use std::path::Component;
            let has_dotdot = s.target.components().any(|c| c == Component::ParentDir);
            let note = if has_dotdot {
                " (warning: target contains '..', may escape virtual root)"
            } else {
                " (symlink, follow manually)"
            };
            writeln!(writer, "-> {}{note}", s.target.display()).map_err(|e| {
                ReplError::InvalidArguments {
                    command: "cat".to_string(),
                    message: e.to_string(),
                }
            })
        }
        None => Err(ReplError::PathNotFound { path: resolved }),
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

    fn run(state: &PreviewState, path: &str) -> Result<String, ReplError> {
        let mut buf = Vec::new();
        execute(state, path, &mut buf)?;
        Ok(String::from_utf8(buf).expect("utf-8"))
    }

    fn state_with_fs(fs: VirtualFs) -> PreviewState {
        let mut state = PreviewState::default();
        state.fs = fs;
        state
    }

    // --- Happy path: regular file ---

    #[test]
    fn cat_prints_file_content() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/hello.txt"), file_node(b"Hello, world!"));
        let state = state_with_fs(fs);
        let out = run(&state, "/app/hello.txt").expect("should succeed");
        assert_eq!(out, "Hello, world!");
    }

    #[test]
    fn cat_empty_file_prints_nothing() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/empty.txt"), file_node(b""));
        let state = state_with_fs(fs);
        let out = run(&state, "/app/empty.txt").expect("should succeed");
        assert_eq!(out, "");
    }

    #[test]
    fn cat_multiline_content_preserved() {
        let mut fs = VirtualFs::new();
        fs.insert(
            PathBuf::from("/app/multi.txt"),
            file_node(b"line1\nline2\nline3"),
        );
        let state = state_with_fs(fs);
        let out = run(&state, "/app/multi.txt").expect("should succeed");
        assert_eq!(out, "line1\nline2\nline3");
    }

    // --- Relative path resolution ---

    #[test]
    fn cat_relative_path_resolved_from_cwd() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/readme.md"), file_node(b"# README"));
        let mut state = state_with_fs(fs);
        state.cwd = PathBuf::from("/app");
        let out = run(&state, "readme.md").expect("should succeed");
        assert_eq!(out, "# README");
    }

    // --- Directory error ---

    #[test]
    fn cat_on_directory_returns_not_a_file_error() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        let state = state_with_fs(fs);
        let result = run(&state, "/app");
        assert!(matches!(result, Err(ReplError::NotAFile { .. })));
    }

    // --- Missing path error ---

    #[test]
    fn cat_missing_path_returns_path_not_found() {
        let state = PreviewState::default();
        let result = run(&state, "/nonexistent/file.txt");
        assert!(matches!(result, Err(ReplError::PathNotFound { .. })));
    }

    // --- Symlink ---

    #[test]
    fn cat_symlink_prints_target_note() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/link"), symlink_node("/usr/bin/python3"));
        let state = state_with_fs(fs);
        let out = run(&state, "/app/link").expect("should succeed");
        assert!(
            out.contains("/usr/bin/python3"),
            "must show target, got: {out}"
        );
        assert!(out.contains("symlink"), "must mention symlink, got: {out}");
    }

    // --- Non-UTF-8 bytes (lossy conversion) ---

    #[test]
    fn cat_non_utf8_content_uses_lossy_conversion() {
        let mut fs = VirtualFs::new();
        // 0xFF is not valid UTF-8; String::from_utf8_lossy replaces it with U+FFFD.
        fs.insert(PathBuf::from("/app/binary"), file_node(b"\xFF\xFE"));
        let state = state_with_fs(fs);
        let out = run(&state, "/app/binary").expect("cat should succeed with lossy conversion");
        // Replacement character U+FFFD must appear in the output.
        assert!(
            out.contains('\u{FFFD}'),
            "non-UTF-8 bytes must produce U+FFFD replacement chars, got: {out:?}"
        );
    }

    // --- Symlink with escaping target warns the user ---

    #[test]
    fn cat_symlink_with_dotdot_target_emits_escape_warning() {
        let mut fs = VirtualFs::new();
        // A symlink whose target contains `..` components could be misleading.
        fs.insert(PathBuf::from("/app/link"), symlink_node("../../etc/passwd"));
        let state = state_with_fs(fs);
        let out = run(&state, "/app/link").expect("should succeed");
        assert!(
            out.contains("warning"),
            "escaping symlink target must show warning, got: {out}"
        );
        assert!(
            out.contains("../../etc/passwd"),
            "target path must be shown, got: {out}"
        );
    }

    // --- Empty path argument returns InvalidArguments ---

    #[test]
    fn cat_empty_path_returns_invalid_arguments() {
        let state = PreviewState::default();
        let result = run(&state, "");
        assert!(
            matches!(result, Err(ReplError::InvalidArguments { ref command, .. }) if command == "cat"),
            "cat with empty path must return InvalidArguments, got: {result:?}"
        );
    }
}
