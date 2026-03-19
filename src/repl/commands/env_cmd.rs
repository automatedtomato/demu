// `env` command — print all environment variables.
//
// Iterates `state.env` (a `BTreeMap` so keys are already sorted lexicographically)
// and prints each entry in `KEY=value` format, one per line.
//
// Both key and value are sanitized via `sanitize_for_terminal` before output
// to prevent terminal escape-sequence injection from user-controlled Dockerfile
// `ENV` instructions.

use std::io::Write;

use crate::model::state::PreviewState;
use crate::output::sanitize::sanitize_for_terminal;
use crate::repl::error::{io_err_mapper, ReplError};

/// Execute the `env` command.
///
/// Writes each environment variable as `KEY=value\n` to `writer`.
/// Keys are emitted in sorted order because `PreviewState.env` is a `BTreeMap`.
/// If the environment is empty, nothing is written.
///
/// Both key and value are passed through `sanitize_for_terminal` before
/// writing. This prevents ANSI escape injection when Dockerfile `ENV`
/// instructions contain control characters.
pub fn execute(state: &PreviewState, writer: &mut impl Write) -> Result<(), ReplError> {
    // Map I/O errors into a uniform ReplError using the shared helper.
    let io_err = io_err_mapper("env");

    for (key, value) in &state.env {
        // Sanitize both key and value: ENV values come from user-controlled
        // Dockerfile text and may contain terminal control sequences.
        let safe_key = sanitize_for_terminal(key);
        let safe_value = sanitize_for_terminal(value);
        writeln!(writer, "{safe_key}={safe_value}").map_err(&io_err)?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::state::PreviewState;

    fn run(state: &PreviewState) -> String {
        let mut buf = Vec::new();
        execute(state, &mut buf).expect("env should not fail");
        String::from_utf8(buf).expect("utf-8")
    }

    // --- Empty environment ---

    #[test]
    fn env_with_empty_state_prints_nothing() {
        let state = PreviewState::default();
        assert_eq!(run(&state), "");
    }

    // --- Single variable ---

    #[test]
    fn env_single_var_printed_as_key_equals_value() {
        let mut state = PreviewState::default();
        state.env.insert("PATH".to_string(), "/usr/bin".to_string());
        let out = run(&state);
        assert!(out.contains("PATH=/usr/bin"), "got: {out}");
    }

    // --- Multiple variables ---

    #[test]
    fn env_multiple_vars_all_present() {
        let mut state = PreviewState::default();
        state.env.insert("B_VAR".to_string(), "beta".to_string());
        state.env.insert("A_VAR".to_string(), "alpha".to_string());
        let out = run(&state);
        assert!(out.contains("A_VAR=alpha"), "got: {out}");
        assert!(out.contains("B_VAR=beta"), "got: {out}");
    }

    #[test]
    fn env_vars_are_sorted_lexicographically() {
        let mut state = PreviewState::default();
        state.env.insert("Z_KEY".to_string(), "z".to_string());
        state.env.insert("A_KEY".to_string(), "a".to_string());
        state.env.insert("M_KEY".to_string(), "m".to_string());
        let out = run(&state);
        let a_pos = out.find("A_KEY").expect("A_KEY must appear");
        let m_pos = out.find("M_KEY").expect("M_KEY must appear");
        let z_pos = out.find("Z_KEY").expect("Z_KEY must appear");
        assert!(a_pos < m_pos, "A_KEY must come before M_KEY");
        assert!(m_pos < z_pos, "M_KEY must come before Z_KEY");
    }

    // --- Values containing `=` ---

    #[test]
    fn env_value_containing_equals_sign_is_preserved() {
        let mut state = PreviewState::default();
        state
            .env
            .insert("GREETING".to_string(), "key=value".to_string());
        let out = run(&state);
        // The entire value including `=` must appear after the key's `=`.
        assert!(out.contains("GREETING=key=value"), "got: {out}");
    }

    // --- Empty value ---

    #[test]
    fn env_empty_value_prints_key_equals_nothing() {
        let mut state = PreviewState::default();
        state.env.insert("EMPTY".to_string(), String::new());
        let out = run(&state);
        assert!(out.contains("EMPTY="), "got: {out}");
    }

    // --- Return value ---

    #[test]
    fn env_returns_ok() {
        let state = PreviewState::default();
        let mut buf = Vec::new();
        assert!(execute(&state, &mut buf).is_ok());
    }

    // --- Sanitization: key ---

    /// A key containing an ANSI escape sequence must have the control bytes
    /// stripped before the line is written to the terminal output buffer.
    /// The raw ESC byte (0x1B) must not appear in the output, but the
    /// non-control portion of the key must still be present.
    #[test]
    fn env_sanitizes_embedded_control_characters_in_key() {
        let mut state = PreviewState::default();
        // Key contains an ANSI erase-display sequence and an embedded newline.
        let poisoned_key = "BAD_KEY\x1b[2J\nEXTRA".to_string();
        state.env.insert(poisoned_key, "safe_value".to_string());
        let mut buf = Vec::new();
        execute(&state, &mut buf).expect("should succeed");
        // ESC byte (0x1B) must be stripped.
        assert!(
            !buf.contains(&0x1B),
            "ESC must be stripped from key output; raw bytes: {:?}",
            buf
        );
        // Embedded newline (0x0A) that was part of the key must also be stripped —
        // only the trailing newline added by writeln! should remain.
        let out = String::from_utf8(buf).expect("utf-8");
        // The safe portion of the key must survive.
        assert!(
            out.contains("BAD_KEY"),
            "base key text must survive sanitization; got:\n{out}"
        );
        // The raw escape sequence characters must not appear literally.
        assert!(
            !out.contains("\x1b"),
            "literal ESC must not appear in output; got:\n{out}"
        );
    }

    // --- Sanitization: value ---

    /// A value containing an ANSI escape sequence must have the control bytes
    /// stripped before the line is written to the terminal output buffer.
    #[test]
    fn env_sanitizes_embedded_control_characters_in_value() {
        let mut state = PreviewState::default();
        // Value contains an ANSI erase-display sequence.
        let poisoned_value = "safe\x1b[2Jvalue".to_string();
        state.env.insert("CLEAN_KEY".to_string(), poisoned_value);
        let mut buf = Vec::new();
        execute(&state, &mut buf).expect("should succeed");
        // ESC byte (0x1B) must be stripped from output.
        assert!(
            !buf.contains(&0x1B),
            "ESC must be stripped from value output; raw bytes: {:?}",
            buf
        );
        let out = String::from_utf8(buf).expect("utf-8");
        // The safe portions of the value must survive.
        assert!(
            out.contains("safe"),
            "leading safe text must survive sanitization; got:\n{out}"
        );
        assert!(
            out.contains("value"),
            "trailing safe text must survive sanitization; got:\n{out}"
        );
        // Key must also appear unchanged.
        assert!(out.contains("CLEAN_KEY"), "key must appear; got:\n{out}");
    }
}
