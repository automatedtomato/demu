// `:installed` command — display packages recorded in the installed registry.
//
// Reads `PreviewState::installed` and prints a grouped summary, one line per
// package manager, listing all recorded package names separated by commas.
// Managers are displayed in a fixed canonical order: apt, pip, npm, apk, go.
// Managers with no recorded packages are silently skipped.
//
// If ALL managers are empty, prints `"No packages recorded."` instead.

use std::io::Write;

use crate::model::state::PreviewState;
use crate::output::sanitize::sanitize_for_terminal;
use crate::repl::error::{io_err_mapper, ReplError};

/// Canonical display order for package managers.
///
/// The order reflects installation frequency in typical Dockerfiles:
/// apt is most common, then pip, then npm, then apk, then go.
/// The label for `go_pkgs` is `"go"` to match the manager name used in
/// `InstalledRegistry::record` and `InstalledRegistry::list`.
const MANAGER_ORDER: &[&str] = &["apt", "pip", "npm", "apk", "go"];

/// Execute the `:installed` command.
///
/// Prints one line per non-empty package manager in the form:
///
/// ```text
/// apt: curl, git, wget
/// pip: flask, requests
/// ```
///
/// Package names within each line are already sorted alphabetically because
/// `InstalledRegistry` uses `BTreeSet` internally.
///
/// When no packages are recorded at all, prints `"No packages recorded."`.
///
/// All output goes to `writer`; I/O errors are mapped to
/// [`ReplError::InvalidArguments`].
pub fn execute(state: &PreviewState, writer: &mut impl Write) -> Result<(), ReplError> {
    // Map I/O errors into a uniform ReplError.
    let io_err = io_err_mapper(":installed");

    // Collect non-empty managers in canonical order.
    let mut any_printed = false;
    for &manager in MANAGER_ORDER {
        let packages = state.installed.list(manager);
        if packages.is_empty() {
            continue;
        }

        // Sanitize each package name before embedding it in terminal output.
        // Package names come from Dockerfile RUN commands, which are
        // user-supplied and may in theory contain control characters.
        let sanitized: Vec<String> = packages.iter().map(|p| sanitize_for_terminal(p)).collect();

        writeln!(writer, "{}: {}", manager, sanitized.join(", ")).map_err(&io_err)?;
        any_printed = true;
    }

    if !any_printed {
        writeln!(writer, "No packages recorded.").map_err(&io_err)?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::state::PreviewState;

    /// Helper: run execute and return captured output as String.
    fn run(state: &PreviewState) -> String {
        let mut buf = Vec::new();
        execute(state, &mut buf).expect(":installed should not fail");
        String::from_utf8(buf).expect("output must be utf-8")
    }

    // --- Empty registry ---

    #[test]
    fn empty_registry_prints_no_packages_message() {
        let state = PreviewState::default();
        let out = run(&state);
        assert_eq!(out.trim(), "No packages recorded.");
    }

    // --- Single manager ---

    #[test]
    fn single_manager_with_packages_shows_correct_output() {
        let mut state = PreviewState::default();
        state.installed.record("apt", "curl".to_string());
        state.installed.record("apt", "git".to_string());
        let out = run(&state);
        assert_eq!(out.trim(), "apt: curl, git");
    }

    // --- Multiple managers appear in correct order ---

    #[test]
    fn multiple_managers_show_in_correct_order() {
        // apt should appear before pip, pip before npm, npm before apk, apk before go.
        let mut state = PreviewState::default();
        state.installed.record("pip", "flask".to_string());
        state.installed.record("apt", "curl".to_string());
        state.installed.record("npm", "express".to_string());
        let out = run(&state);

        let apt_pos = out.find("apt:").expect("apt must appear");
        let pip_pos = out.find("pip:").expect("pip must appear");
        let npm_pos = out.find("npm:").expect("npm must appear");

        assert!(apt_pos < pip_pos, "apt must come before pip; got:\n{out}");
        assert!(pip_pos < npm_pos, "pip must come before npm; got:\n{out}");
    }

    // --- Empty managers are skipped ---

    #[test]
    fn empty_managers_are_skipped() {
        let mut state = PreviewState::default();
        // Only pip has packages; apt, npm, apk, go are all empty.
        state.installed.record("pip", "requests".to_string());
        let out = run(&state);
        assert!(!out.contains("apt:"), "apt must not appear; got:\n{out}");
        assert!(out.contains("pip:"), "pip must appear; got:\n{out}");
        assert!(!out.contains("npm:"), "npm must not appear; got:\n{out}");
        assert!(!out.contains("apk:"), "apk must not appear; got:\n{out}");
        assert!(!out.contains("go:"), "go must not appear; got:\n{out}");
    }

    // --- only_non_empty_managers_appear (alias of above, different fixture) ---

    #[test]
    fn only_non_empty_managers_appear() {
        let mut state = PreviewState::default();
        state.installed.record("npm", "lodash".to_string());
        state
            .installed
            .record("go", "golang.org/x/tools".to_string());
        let out = run(&state);
        assert!(!out.contains("apt:"), "got:\n{out}");
        assert!(!out.contains("pip:"), "got:\n{out}");
        assert!(out.contains("npm:"), "got:\n{out}");
        assert!(!out.contains("apk:"), "got:\n{out}");
        assert!(out.contains("go:"), "got:\n{out}");
    }

    // --- All five managers in correct order ---

    #[test]
    fn all_five_managers_displayed_in_order() {
        let mut state = PreviewState::default();
        state.installed.record("apt", "curl".to_string());
        state.installed.record("pip", "flask".to_string());
        state.installed.record("npm", "express".to_string());
        state.installed.record("apk", "bash".to_string());
        state
            .installed
            .record("go", "golang.org/x/tools".to_string());
        let out = run(&state);

        let apt_pos = out.find("apt:").expect("apt must appear");
        let pip_pos = out.find("pip:").expect("pip must appear");
        let npm_pos = out.find("npm:").expect("npm must appear");
        let apk_pos = out.find("apk:").expect("apk must appear");
        let go_pos = out.find("go:").expect("go must appear");

        assert!(apt_pos < pip_pos, "apt before pip; got:\n{out}");
        assert!(pip_pos < npm_pos, "pip before npm; got:\n{out}");
        assert!(npm_pos < apk_pos, "npm before apk; got:\n{out}");
        assert!(apk_pos < go_pos, "apk before go; got:\n{out}");
    }

    // --- go_pkgs is displayed under the "go" label ---

    #[test]
    fn go_pkgs_displayed_as_go_label() {
        let mut state = PreviewState::default();
        // `record("go", ...)` inserts into the `go_pkgs` BTreeSet internally.
        state
            .installed
            .record("go", "github.com/user/pkg".to_string());
        let out = run(&state);
        assert!(
            out.contains("go: github.com/user/pkg"),
            "go packages must appear under 'go:' label; got:\n{out}"
        );
    }

    // --- Sanitization ---

    #[test]
    fn package_names_with_escape_sequences_are_sanitized() {
        let mut state = PreviewState::default();
        // Inject an ANSI escape sequence into a package name.
        state.installed.apt.insert("curl\x1b[2J".to_string());
        let buf = {
            let mut b = Vec::new();
            execute(&state, &mut b).expect("should succeed");
            b
        };
        // ESC byte (0x1B) must not appear in terminal output.
        assert!(
            !buf.contains(&0x1B),
            "ESC must be stripped from package name output"
        );
        // The base text should still appear (sans the escape sequence).
        let out = String::from_utf8(buf).expect("utf-8");
        assert!(
            out.contains("curl"),
            "base name must survive sanitization; got:\n{out}"
        );
    }

    // --- Alphabetical ordering within a manager ---

    #[test]
    fn packages_within_manager_are_listed_alphabetically() {
        // Insert packages in reverse alphabetical order to confirm that
        // the BTreeSet sort guarantee flows through the formatter.
        let mut state = PreviewState::default();
        state.installed.record("apt", "wget".to_string());
        state.installed.record("apt", "curl".to_string());
        state.installed.record("apt", "bash".to_string());
        let out = run(&state);
        assert!(
            out.contains("apt: bash, curl, wget"),
            "packages must be listed in alphabetical order; got:\n{out}"
        );
    }
}
