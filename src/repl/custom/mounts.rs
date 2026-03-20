// `:mounts` command — list all volume mount shadows applied by the Compose engine.
//
// In Compose mode, `run_compose` populates `state.mounts` with one `MountInfo`
// per volume entry declared in the selected service.  This command renders that
// list in a compact, terminal-friendly format:
//
//   Mount shadows (2 total):
//     /data           bind mount from ./data  [rw]
//     /root/.npm      named volume: npm-cache  [ro]
//
// When no mounts have been recorded (e.g. in plain Dockerfile mode or when the
// Compose service declares no volumes), a brief sentinel line is printed.

use std::io::Write;

use crate::model::state::PreviewState;
use crate::output::sanitize::sanitize_for_terminal;
use crate::repl::error::ReplError;

/// Execute the `:mounts` command.
///
/// Writes a formatted list of all volume mount shadows to `writer`.
/// When `state.mounts` is empty, prints `"No mount shadows recorded."`.
pub fn execute(state: &PreviewState, writer: &mut impl Write) -> Result<(), ReplError> {
    let io_err = |e: std::io::Error| ReplError::InvalidArguments {
        command: ":mounts".to_string(),
        message: e.to_string(),
    };

    if state.mounts.is_empty() {
        writeln!(writer, "No mount shadows recorded.").map_err(io_err)?;
        return Ok(());
    }

    writeln!(writer, "Mount shadows ({} total):", state.mounts.len()).map_err(io_err)?;

    for mount in &state.mounts {
        // Sanitize user-supplied data before writing to the terminal.
        let safe_path = sanitize_for_terminal(&mount.container_path.display().to_string());
        let safe_desc = sanitize_for_terminal(&mount.description);
        let rw_label = if mount.read_only { "ro" } else { "rw" };

        writeln!(writer, "  {safe_path}  {safe_desc}  [{rw_label}]").map_err(io_err)?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::model::provenance::MountInfo;
    use crate::model::state::PreviewState;
    use std::path::PathBuf;

    fn run(state: &PreviewState) -> String {
        let mut buf = Vec::new();
        execute(state, &mut buf).expect("mounts should not fail");
        String::from_utf8(buf).expect("utf-8")
    }

    // --- empty state ---

    #[test]
    fn empty_mounts_prints_sentinel() {
        let state = PreviewState::default();
        let out = run(&state);
        assert_eq!(out.trim(), "No mount shadows recorded.", "got: {out}");
    }

    // --- single bind mount ---

    #[test]
    fn bind_mount_shows_container_path_and_description() {
        let mut state = PreviewState::default();
        state.mounts.push(MountInfo {
            container_path: PathBuf::from("/data"),
            read_only: false,
            description: "bind mount from ./data".to_string(),
        });
        let out = run(&state);
        assert!(
            out.contains("/data"),
            "must show container path; got: {out}"
        );
        assert!(
            out.contains("bind mount from ./data"),
            "must show description; got: {out}"
        );
        assert!(out.contains("[rw]"), "must show rw label; got: {out}");
    }

    // --- read-only mount ---

    #[test]
    fn read_only_mount_shows_ro_label() {
        let mut state = PreviewState::default();
        state.mounts.push(MountInfo {
            container_path: PathBuf::from("/config"),
            read_only: true,
            description: "named volume: cfg".to_string(),
        });
        let out = run(&state);
        assert!(
            out.contains("[ro]"),
            "read-only mount must show [ro]; got: {out}"
        );
    }

    // --- named volume ---

    #[test]
    fn named_volume_shows_volume_name_in_description() {
        let mut state = PreviewState::default();
        state.mounts.push(MountInfo {
            container_path: PathBuf::from("/root/.npm"),
            read_only: false,
            description: "named volume: npm-cache".to_string(),
        });
        let out = run(&state);
        assert!(
            out.contains("named volume: npm-cache"),
            "must show named volume; got: {out}"
        );
    }

    // --- anonymous volume ---

    #[test]
    fn anonymous_volume_shows_description() {
        let mut state = PreviewState::default();
        state.mounts.push(MountInfo {
            container_path: PathBuf::from("/tmp/scratch"),
            read_only: false,
            description: "anonymous volume".to_string(),
        });
        let out = run(&state);
        assert!(
            out.contains("anonymous volume"),
            "must show anonymous volume; got: {out}"
        );
    }

    // --- total count in header ---

    #[test]
    fn header_shows_correct_count() {
        let mut state = PreviewState::default();
        state.mounts.push(MountInfo {
            container_path: PathBuf::from("/data"),
            read_only: false,
            description: "bind mount from ./data".to_string(),
        });
        state.mounts.push(MountInfo {
            container_path: PathBuf::from("/cache"),
            read_only: false,
            description: "named volume: cache".to_string(),
        });
        let out = run(&state);
        assert!(
            out.contains("Mount shadows (2 total):"),
            "header must show count; got: {out}"
        );
    }

    // --- ANSI sanitization ---

    #[test]
    fn ansi_escape_in_description_is_stripped() {
        let mut state = PreviewState::default();
        state.mounts.push(MountInfo {
            container_path: PathBuf::from("/data"),
            read_only: false,
            description: "bind mount from \x1b[2J./data".to_string(),
        });
        let out = run(&state);
        assert!(
            !out.contains('\x1b'),
            "ANSI escapes must be stripped; got: {out:?}"
        );
    }
}
