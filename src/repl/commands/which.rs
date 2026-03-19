// `which` command — show the simulated binary path for a command name.
//
// Searches the installed package registry in a fixed priority order:
// apt → apk → pip → npm → go_pkgs (first match wins).
//
// Packages installed via apt or apk simulate the standard Linux system path
// (`/usr/bin/<cmd>`), while packages installed via pip, npm, or go simulate
// the local bin path (`/usr/local/bin/<cmd>`).
//
// This is an approximation: demu does not know which binaries a package
// actually provides. It assumes the command name matches a recorded package
// name. This approximation is surfaced by the nature of demu as a preview tool.

use std::io::Write;
use std::path::PathBuf;

use crate::model::state::PreviewState;
use crate::output::sanitize::sanitize_for_terminal;
use crate::repl::error::{io_err_mapper, ReplError};

/// Execute the `which <cmd>` command.
///
/// Searches the installed package registry for `cmd` and prints the simulated
/// binary path. Returns [`ReplError::InvalidArguments`] if `cmd` is empty,
/// and [`ReplError::PathNotFound`] if no matching package is found.
///
/// Search order (first match wins): apt → apk → pip → npm → go_pkgs.
/// - apt / apk → `/usr/bin/<cmd>`
/// - pip / npm / go_pkgs → `/usr/local/bin/<cmd>`
pub fn execute(state: &PreviewState, cmd: &str, writer: &mut impl Write) -> Result<(), ReplError> {
    // Reject empty command name before any registry lookup.
    if cmd.is_empty() {
        return Err(ReplError::InvalidArguments {
            command: "which".to_string(),
            message: "missing command name".to_string(),
        });
    }

    // Sanitize the command name for output only. Registry lookups use the raw `cmd`
    // so they match however the engine stored the package name (also unsanitized).
    // The two strings intentionally diverge: lookup key is raw, printed path is safe.
    let safe_cmd = sanitize_for_terminal(cmd);

    // Delegate priority-ordered search to InstalledRegistry::which_prefix so that
    // the search order and path-prefix mapping live in exactly one place.
    let prefix = state
        .installed
        .which_prefix(cmd)
        .ok_or_else(|| ReplError::PathNotFound {
            path: PathBuf::from(cmd),
        })?;

    let path = format!("{prefix}/{safe_cmd}");
    writeln!(writer, "{path}").map_err(io_err_mapper("which"))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::state::PreviewState;

    /// Helper: run execute with a given cmd and return captured output.
    fn run(state: &PreviewState, cmd: &str) -> Result<String, ReplError> {
        let mut buf = Vec::new();
        execute(state, cmd, &mut buf)?;
        Ok(String::from_utf8(buf).expect("output must be utf-8"))
    }

    // --- Empty cmd ---

    #[test]
    fn empty_cmd_returns_invalid_arguments() {
        let state = PreviewState::default();
        let result = run(&state, "");
        assert!(
            matches!(result, Err(ReplError::InvalidArguments { ref command, .. }) if command == "which"),
            "empty cmd must return InvalidArguments for 'which'; got: {result:?}"
        );
    }

    // --- apt package ---

    #[test]
    fn apt_package_returns_usr_bin_path() {
        let mut state = PreviewState::default();
        state.installed.record("apt", "curl".to_string());
        let out = run(&state, "curl").expect("should find curl");
        assert_eq!(out.trim(), "/usr/bin/curl");
    }

    // --- apk package ---

    #[test]
    fn apk_package_returns_usr_bin_path() {
        let mut state = PreviewState::default();
        state.installed.record("apk", "wget".to_string());
        let out = run(&state, "wget").expect("should find wget");
        assert_eq!(out.trim(), "/usr/bin/wget");
    }

    // --- pip package ---

    #[test]
    fn pip_package_returns_usr_local_bin_path() {
        let mut state = PreviewState::default();
        state.installed.record("pip", "flask".to_string());
        let out = run(&state, "flask").expect("should find flask");
        assert_eq!(out.trim(), "/usr/local/bin/flask");
    }

    // --- npm package ---

    #[test]
    fn npm_package_returns_usr_local_bin_path() {
        let mut state = PreviewState::default();
        state.installed.record("npm", "nodemon".to_string());
        let out = run(&state, "nodemon").expect("should find nodemon");
        assert_eq!(out.trim(), "/usr/local/bin/nodemon");
    }

    // --- go package ---

    #[test]
    fn go_package_returns_usr_local_bin_path() {
        let mut state = PreviewState::default();
        state.installed.record("go", "gopls".to_string());
        let out = run(&state, "gopls").expect("should find gopls");
        assert_eq!(out.trim(), "/usr/local/bin/gopls");
    }

    // --- Not found ---

    #[test]
    fn not_found_returns_path_not_found() {
        let state = PreviewState::default();
        let result = run(&state, "nonexistent");
        assert!(
            matches!(result, Err(ReplError::PathNotFound { ref path }) if path == &PathBuf::from("nonexistent")),
            "unrecorded cmd must return PathNotFound; got: {result:?}"
        );
    }

    // --- Priority: search-order collision tests ---

    #[test]
    fn apt_takes_priority_over_pip() {
        let mut state = PreviewState::default();
        state.installed.record("pip", "git".to_string());
        state.installed.record("apt", "git".to_string());
        // apt is checked first; must return /usr/bin, not /usr/local/bin.
        let out = run(&state, "git").expect("should find git");
        assert_eq!(
            out.trim(),
            "/usr/bin/git",
            "apt takes priority over pip; got: {out}"
        );
    }

    #[test]
    fn apk_takes_priority_over_pip() {
        // apk is in the /usr/bin group; pip is in /usr/local/bin — apk wins.
        let mut state = PreviewState::default();
        state.installed.record("pip", "bash".to_string());
        state.installed.record("apk", "bash".to_string());
        let out = run(&state, "bash").expect("should find bash");
        assert_eq!(
            out.trim(),
            "/usr/bin/bash",
            "apk takes priority over pip; got: {out}"
        );
    }

    #[test]
    fn pip_takes_priority_over_npm() {
        // pip and npm share the /usr/local/bin group; pip is listed first.
        let mut state = PreviewState::default();
        state.installed.record("npm", "requests".to_string());
        state.installed.record("pip", "requests".to_string());
        let out = run(&state, "requests").expect("should find requests");
        assert_eq!(
            out.trim(),
            "/usr/local/bin/requests",
            "pip takes priority over npm; got: {out}"
        );
    }

    // --- Sanitization of cmd ---

    #[test]
    fn cmd_with_escape_sequence_is_sanitized() {
        let mut state = PreviewState::default();
        // Insert the raw key (with escape) so the lookup finds it.
        state.installed.apt.insert("curl\x1b[2J".to_string());
        let buf = {
            let mut b = Vec::new();
            execute(&state, "curl\x1b[2J", &mut b).expect("should succeed");
            b
        };
        // ESC byte (0x1B) must not appear in the printed path.
        assert!(
            !buf.contains(&0x1B),
            "ESC must be stripped from which output"
        );
        // The sanitized output must still contain the base name.
        let out = String::from_utf8(buf).expect("utf-8");
        assert!(
            out.contains("curl"),
            "base name must survive sanitization; got:\n{out}"
        );
    }
}
