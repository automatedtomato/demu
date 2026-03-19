// `pip list` command — show simulated pip packages.
//
// Prints packages recorded in `state.installed.pip` in the same tabular
// format as the real `pip list` command:
//
//   Package    Version
//   ---------- -------
//   requests   (simulated)
//
// This is a simulation: no real package data is fetched. The output comes
// entirely from what the engine recorded during `RUN pip install` processing.

use std::io::Write;

use crate::model::state::PreviewState;
use crate::output::sanitize::sanitize_for_terminal;
use crate::repl::error::ReplError;

/// Column width for the Package name field.
///
/// Matches real `pip list` behaviour: the Package column is at least 10 chars
/// wide. Names longer than 10 chars extend naturally beyond the column.
const PKG_COL_WIDTH: usize = 10;

/// Execute the `pip list` command.
///
/// Always writes the pip table header and separator, then one row per
/// recorded pip package. When the registry is empty, only the header
/// and separator are printed (matching real `pip list` behaviour on an
/// empty environment).
///
/// Package names are sanitized before output to prevent terminal escape injection.
pub fn execute(state: &PreviewState, writer: &mut impl Write) -> Result<(), ReplError> {
    // Map I/O errors into ReplError so callers see a consistent error type.
    let io_err = |e: std::io::Error| ReplError::InvalidArguments {
        command: "pip".to_string(),
        message: e.to_string(),
    };

    // Always write the header and separator, even for an empty registry.
    writeln!(writer, "Package    Version").map_err(io_err)?;
    writeln!(writer, "---------- -------").map_err(io_err)?;

    let packages = state.installed.list("pip");
    for pkg in &packages {
        // Sanitize each package name before writing; package names originate from
        // engine-processed Dockerfile text which may contain escape bytes.
        let safe_pkg = sanitize_for_terminal(pkg);
        writeln!(writer, "{safe_pkg:<PKG_COL_WIDTH$} (simulated)").map_err(io_err)?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::state::PreviewState;

    /// Helper: run execute and capture output as String.
    fn run(state: &PreviewState) -> String {
        let mut buf = Vec::new();
        execute(state, &mut buf).expect("pip execute should not fail");
        String::from_utf8(buf).unwrap()
    }

    // --- Empty registry prints header only ---

    #[test]
    fn pip_list_empty_registry_prints_header_only() {
        let state = PreviewState::default();
        let out = run(&state);
        assert!(
            out.contains("Package    Version"),
            "must contain header; got: {out}"
        );
        assert!(
            out.contains("---------- -------"),
            "must contain separator; got: {out}"
        );
        // No data rows should appear when registry is empty.
        assert!(
            !out.contains("(simulated)"),
            "must not print data rows for empty registry; got: {out}"
        );
    }

    // --- Single package ---

    #[test]
    fn pip_list_single_package() {
        let mut state = PreviewState::default();
        state.installed.record("pip", "requests".to_string());
        let out = run(&state);
        assert!(
            out.contains("Package    Version"),
            "header missing; got: {out}"
        );
        assert!(
            out.contains("---------- -------"),
            "separator missing; got: {out}"
        );
        // Assert the combined, column-aligned row so a split-line regression
        // would not pass. "requests" (8 chars) padded to 10 + 1 space = 3 spaces.
        assert!(
            out.contains("requests   (simulated)"),
            "must show column-aligned row for requests; got: {out}"
        );
    }

    // --- Multiple packages in alphabetical order ---

    #[test]
    fn pip_list_multiple_packages_alphabetical() {
        let mut state = PreviewState::default();
        // Insert in reverse alphabetical order — BTreeSet will sort them.
        state.installed.record("pip", "requests".to_string());
        state.installed.record("pip", "flask".to_string());
        state.installed.record("pip", "django".to_string());
        let out = run(&state);
        // All three must appear.
        assert!(out.contains("django"), "got: {out}");
        assert!(out.contains("flask"), "got: {out}");
        assert!(out.contains("requests"), "got: {out}");
        // Alphabetical order: django < flask < requests.
        let django_pos = out.find("django").expect("django must appear");
        let flask_pos = out.find("flask").expect("flask must appear");
        let requests_pos = out.find("requests").expect("requests must appear");
        assert!(
            django_pos < flask_pos,
            "django must precede flask; got:\n{out}"
        );
        assert!(
            flask_pos < requests_pos,
            "flask must precede requests; got:\n{out}"
        );
    }

    // --- Column alignment for short names ---

    #[test]
    fn pip_list_column_alignment() {
        // "os" is 2 chars. PKG_COL_WIDTH=10 left-pads it to 10 chars with :<10
        // (adding 8 spaces), then the format literal adds one more space before
        // "(simulated)", yielding 9 spaces total between the name and the version.
        let mut state = PreviewState::default();
        state.installed.record("pip", "os".to_string());
        let out = run(&state);
        assert!(
            out.contains("os         (simulated)"),
            "short name must be padded to align with version column; got:\n{out}"
        );
    }

    // --- Escape sequence sanitization ---

    #[test]
    fn pip_list_sanitizes_escape_sequences() {
        let mut state = PreviewState::default();
        // Insert directly into the BTreeSet to bypass record() string handling.
        state.installed.pip.insert("requests\x1b[2J".to_string());
        let buf = {
            let mut b = Vec::new();
            execute(&state, &mut b).expect("should succeed");
            b
        };
        // ESC byte (0x1B) must not appear in the printed output.
        assert!(
            !buf.contains(&0x1B),
            "ESC must be stripped from pip list output"
        );
        // The sanitized base name must still be present.
        let out = String::from_utf8(buf).expect("utf-8");
        assert!(
            out.contains("requests"),
            "base name must survive sanitization; got:\n{out}"
        );
    }
}
