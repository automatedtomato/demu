// `cd` command — change the working directory.
//
// Resolves the given path relative to the current working directory,
// verifies that the target exists and is a directory, then updates
// `state.cwd`. This is the only REPL command that mutates `PreviewState`.

use std::io::Write;

use crate::model::fs::FsNode;
use crate::model::state::PreviewState;
use crate::repl::error::ReplError;
use crate::repl::path::resolve_path;
use std::path::Path;

/// Execute the `cd` command.
///
/// Resolves `path` relative to `state.cwd`, then checks the virtual
/// filesystem:
/// - If the resolved path is `/` (root), always allow it.
/// - If the target is a `Directory`, update `state.cwd`.
/// - If the target is a `File` or `Symlink`, return `ReplError::NotADirectory`.
/// - If the target does not exist, return `ReplError::PathNotFound`.
///
/// `writer` is included for API consistency but no output is produced on success.
pub fn execute(
    state: &mut PreviewState,
    path: &str,
    _writer: &mut impl Write,
) -> Result<(), ReplError> {
    let resolved = resolve_path(&state.cwd, path);

    // Root is always valid — it may not be explicitly stored in the virtual fs.
    if resolved == Path::new("/") {
        state.cwd = resolved;
        return Ok(());
    }

    match state.fs.get(&resolved) {
        Some(FsNode::Directory(_)) => {
            state.cwd = resolved;
            Ok(())
        }
        Some(FsNode::File(_)) | Some(FsNode::Symlink(_)) => {
            Err(ReplError::NotADirectory { path: resolved })
        }
        None => {
            // The VirtualFs stores a flat HashMap. A directory may have children
            // without an explicit DirNode (e.g. files were COPY-ed into a path
            // that was never the target of a WORKDIR). Mirror the ls guard: accept
            // the path as a valid directory if it has any children.
            if !state.fs.list_dir(&resolved).is_empty() {
                state.cwd = resolved;
                Ok(())
            } else {
                Err(ReplError::PathNotFound { path: resolved })
            }
        }
    }
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

    fn run(state: &mut PreviewState, path: &str) -> Result<(), ReplError> {
        let mut buf = Vec::new();
        execute(state, path, &mut buf)
    }

    // --- Absolute paths ---

    #[test]
    fn cd_absolute_path_updates_cwd() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        let mut state = state_with_fs(fs);
        run(&mut state, "/app").expect("cd /app should succeed");
        assert_eq!(state.cwd, PathBuf::from("/app"));
    }

    #[test]
    fn cd_root_always_works() {
        let mut state = PreviewState::default();
        state.cwd = PathBuf::from("/app");
        run(&mut state, "/").expect("cd / should always succeed");
        assert_eq!(state.cwd, PathBuf::from("/"));
    }

    // --- Relative paths ---

    #[test]
    fn cd_relative_path_resolved_from_cwd() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        fs.insert(PathBuf::from("/app/src"), dir_node());
        let mut state = state_with_fs(fs);
        state.cwd = PathBuf::from("/app");
        run(&mut state, "src").expect("cd src should succeed");
        assert_eq!(state.cwd, PathBuf::from("/app/src"));
    }

    // --- dotdot ---

    #[test]
    fn cd_dotdot_moves_up_one_level() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        fs.insert(PathBuf::from("/app/src"), dir_node());
        let mut state = state_with_fs(fs);
        state.cwd = PathBuf::from("/app/src");
        run(&mut state, "..").expect("cd .. should succeed");
        assert_eq!(state.cwd, PathBuf::from("/app"));
    }

    #[test]
    fn cd_dotdot_at_root_stays_at_root() {
        let mut state = PreviewState::default(); // cwd = /
        run(&mut state, "..").expect("cd .. at root should succeed");
        assert_eq!(state.cwd, PathBuf::from("/"));
    }

    // --- Error cases ---

    #[test]
    fn cd_nonexistent_directory_returns_path_not_found() {
        let mut state = PreviewState::default();
        let result = run(&mut state, "/nonexistent");
        assert!(matches!(result, Err(ReplError::PathNotFound { .. })));
    }

    #[test]
    fn cd_into_file_returns_not_a_directory() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/file.txt"), file_node());
        let mut state = state_with_fs(fs);
        let result = run(&mut state, "/app/file.txt");
        assert!(matches!(result, Err(ReplError::NotADirectory { .. })));
    }

    // --- CWD unchanged on error ---

    #[test]
    fn cd_cwd_unchanged_on_error() {
        let mut state = PreviewState::default();
        state.cwd = PathBuf::from("/");
        let _ = run(&mut state, "/nonexistent");
        assert_eq!(
            state.cwd,
            PathBuf::from("/"),
            "cwd must not change on error"
        );
    }

    // --- No output on success ---

    #[test]
    fn cd_produces_no_output_on_success() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        let mut state = state_with_fs(fs);
        let mut buf = Vec::new();
        execute(&mut state, "/app", &mut buf).expect("should succeed");
        assert_eq!(buf, b"", "cd should produce no output");
    }
}
