// `apt list --installed` command — show simulated apt packages.
//
// Prints packages recorded in `state.installed.apt` using Debian-style
// `apt list --installed` output format. When the `--installed` flag is not
// given, prints a usage hint instead.
//
// This is a simulation: no real package resolution is performed. The output
// is derived entirely from what the engine recorded during `RUN apt-get install`
// processing.

use std::io::Write;

use crate::model::state::PreviewState;
use crate::output::sanitize::sanitize_for_terminal;
use crate::repl::error::{io_err_mapper, ReplError};

/// Execute the `apt list [--installed]` command.
///
/// When `installed` is `false`, prints a usage hint and returns early.
/// When `installed` is `true`, prints the Debian-style listing of all
/// simulated apt packages from the registry, one per line:
///
/// ```text
/// Listing...
/// <package>/simulated [installed,simulated]
/// ```
///
/// If no packages are recorded, prints `"(no packages recorded)\n"` instead
/// of an empty listing, to match the real `apt list --installed` sentinel.
///
/// Package names are sanitized before output to prevent terminal escape injection.
pub fn execute(
    state: &PreviewState,
    installed: bool,
    writer: &mut impl Write,
) -> Result<(), ReplError> {
    // Map I/O errors into ReplError so callers see a consistent error type.
    let io_err = io_err_mapper("apt");

    if !installed {
        // No --installed flag — show usage hint, not the full listing.
        writeln!(writer, "Usage: apt list --installed").map_err(&io_err)?;
        return Ok(());
    }

    // --installed flag given — produce the Debian-style listing.
    writeln!(writer, "Listing...").map_err(&io_err)?;

    let packages = state.installed.list("apt");
    if packages.is_empty() {
        writeln!(writer, "(no packages recorded)").map_err(&io_err)?;
    } else {
        for pkg in &packages {
            // Sanitize each package name before writing; package names come
            // from engine-processed Dockerfile text which may contain escape bytes.
            let safe_pkg = sanitize_for_terminal(pkg);
            writeln!(writer, "{safe_pkg}/simulated [installed,simulated]").map_err(&io_err)?;
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::state::PreviewState;

    /// Helper: run execute with --installed flag and capture output as String.
    fn run_installed(state: &PreviewState) -> String {
        let mut buf = Vec::new();
        execute(state, true, &mut buf).expect("apt execute should not fail");
        String::from_utf8(buf).unwrap()
    }

    /// Helper: run execute without --installed flag and capture output as String.
    fn run_no_flag(state: &PreviewState) -> String {
        let mut buf = Vec::new();
        execute(state, false, &mut buf).expect("apt execute should not fail");
        String::from_utf8(buf).unwrap()
    }

    // --- Empty registry ---

    #[test]
    fn apt_list_installed_empty_registry() {
        let state = PreviewState::default();
        let out = run_installed(&state);
        assert!(
            out.contains("Listing..."),
            "must start with 'Listing...'; got: {out}"
        );
        assert!(
            out.contains("(no packages recorded)"),
            "empty registry must print sentinel; got: {out}"
        );
    }

    // --- Single package ---

    #[test]
    fn apt_list_installed_single_package() {
        let mut state = PreviewState::default();
        state.installed.record("apt", "curl".to_string());
        let out = run_installed(&state);
        assert!(out.contains("Listing..."), "got: {out}");
        assert!(
            out.contains("curl/simulated [installed,simulated]"),
            "must contain Debian-style line for curl; got: {out}"
        );
        // Empty-registry sentinel must NOT appear when there are packages.
        assert!(
            !out.contains("(no packages recorded)"),
            "sentinel must not appear when packages are present; got: {out}"
        );
    }

    // --- Multiple packages in alphabetical order ---

    #[test]
    fn apt_list_installed_multiple_packages_alphabetical() {
        let mut state = PreviewState::default();
        // Insert in reverse alphabetical order — BTreeSet will sort them.
        state.installed.record("apt", "wget".to_string());
        state.installed.record("apt", "bash".to_string());
        state.installed.record("apt", "curl".to_string());
        let out = run_installed(&state);
        // Verify all three appear.
        assert!(
            out.contains("bash/simulated [installed,simulated]"),
            "got: {out}"
        );
        assert!(
            out.contains("curl/simulated [installed,simulated]"),
            "got: {out}"
        );
        assert!(
            out.contains("wget/simulated [installed,simulated]"),
            "got: {out}"
        );
        // Verify alphabetical order: bash < curl < wget.
        let bash_pos = out.find("bash/simulated").expect("bash must appear");
        let curl_pos = out.find("curl/simulated").expect("curl must appear");
        let wget_pos = out.find("wget/simulated").expect("wget must appear");
        assert!(
            bash_pos < curl_pos,
            "bash must come before curl; got:\n{out}"
        );
        assert!(
            curl_pos < wget_pos,
            "curl must come before wget; got:\n{out}"
        );
    }

    // --- No --installed flag prints usage ---

    #[test]
    fn apt_list_no_flag_prints_usage() {
        let state = PreviewState::default();
        let out = run_no_flag(&state);
        assert!(
            out.contains("Usage: apt list --installed"),
            "must print usage hint when flag omitted; got: {out}"
        );
        // Must NOT print the listing header.
        assert!(
            !out.contains("Listing..."),
            "must not print listing header when flag omitted; got: {out}"
        );
    }

    // --- Escape sequence and control character sanitization ---

    #[test]
    fn apt_list_sanitizes_embedded_newline_in_package_name() {
        // An embedded \n in a package name must not produce an extra output line.
        // sanitize_for_terminal strips C0 control characters including LF (0x0A).
        let mut state = PreviewState::default();
        state.installed.apt.insert("curl\necho pwned".to_string());
        let buf = {
            let mut b = Vec::new();
            execute(&state, true, &mut b).expect("should succeed");
            b
        };
        let out = String::from_utf8(buf).expect("utf-8");
        // Only one data line should appear (the "Listing..." header is the other line).
        let data_lines: Vec<&str> = out.lines().filter(|l| l.contains("/simulated")).collect();
        assert_eq!(
            data_lines.len(),
            1,
            "embedded newline must not produce extra output lines; got:\n{out}"
        );
    }

    #[test]
    fn apt_list_sanitizes_escape_sequences() {
        let mut state = PreviewState::default();
        // Insert directly into the BTreeSet to bypass record() string handling.
        state.installed.apt.insert("curl\x1b[2J".to_string());
        let buf = {
            let mut b = Vec::new();
            execute(&state, true, &mut b).expect("should succeed");
            b
        };
        // ESC byte (0x1B) must not appear in the printed output.
        assert!(
            !buf.contains(&0x1B),
            "ESC must be stripped from apt list output"
        );
        // The sanitized base name must still be present.
        let out = String::from_utf8(buf).expect("utf-8");
        assert!(
            out.contains("curl"),
            "base name must survive sanitization; got:\n{out}"
        );
    }
}
