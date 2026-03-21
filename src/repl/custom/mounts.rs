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
// In single-Dockerfile mode (no ComposeContext), prints a clear usage hint.

use std::io::Write;

use crate::model::state::PreviewState;
use crate::output::sanitize::sanitize_for_terminal;
use crate::repl::config::ComposeContext;
use crate::repl::error::ReplError;

/// Guard message printed when `:mounts` is invoked outside Compose mode.
const GUARD_MSG: &str = "\
:mounts is only available in compose mode
  Usage: demu --compose -f compose.yaml --service <name>
";

/// Execute the `:mounts` command.
///
/// When `compose_ctx` is `None` the guard message is printed.
/// Otherwise, writes a formatted list of all volume mount shadows to `writer`.
pub fn execute(
    state: &PreviewState,
    compose_ctx: Option<&ComposeContext>,
    writer: &mut impl Write,
) -> Result<(), ReplError> {
    let io_err = |e: std::io::Error| ReplError::Io {
        command: ":mounts".to_string(),
        message: e.to_string(),
    };

    if compose_ctx.is_none() {
        write!(writer, "{GUARD_MSG}").map_err(io_err)?;
        return Ok(());
    }

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
    use crate::model::compose::ComposeFile;
    use crate::model::provenance::MountInfo;
    use crate::model::state::PreviewState;
    use crate::repl::config::ComposeContext;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn dummy_ctx() -> ComposeContext {
        ComposeContext {
            compose_file: ComposeFile {
                services: BTreeMap::new(),
                volumes: BTreeMap::new(),
            },
            selected_service: "svc".to_string(),
        }
    }

    fn run(state: &PreviewState, ctx: Option<&ComposeContext>) -> String {
        let mut buf = Vec::new();
        execute(state, ctx, &mut buf).expect("mounts should not fail");
        String::from_utf8(buf).expect("utf-8")
    }

    // --- guard mode ---

    #[test]
    fn no_context_prints_guard_message() {
        let state = PreviewState::default();
        let out = run(&state, None);
        assert!(
            out.contains(":mounts is only available in compose mode"),
            "guard message missing; got: {out}"
        );
    }

    // --- empty state ---

    #[test]
    fn empty_mounts_prints_sentinel() {
        let ctx = dummy_ctx();
        let state = PreviewState::default();
        let out = run(&state, Some(&ctx));
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
        let ctx = dummy_ctx();
        let out = run(&state, Some(&ctx));
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
        let ctx = dummy_ctx();
        let out = run(&state, Some(&ctx));
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
        let ctx = dummy_ctx();
        let out = run(&state, Some(&ctx));
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
        let ctx = dummy_ctx();
        let out = run(&state, Some(&ctx));
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
        let ctx = dummy_ctx();
        let out = run(&state, Some(&ctx));
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
        let ctx = dummy_ctx();
        let out = run(&state, Some(&ctx));
        assert!(
            !out.contains('\x1b'),
            "ANSI escapes must be stripped; got: {out:?}"
        );
    }
}
