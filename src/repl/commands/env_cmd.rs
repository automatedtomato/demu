// `env` command — print all environment variables.
//
// Iterates `state.env` (a `BTreeMap` so keys are already sorted lexicographically)
// and prints each entry in `KEY=value` format, one per line.

use std::io::Write;

use crate::model::state::PreviewState;
use crate::repl::error::ReplError;

/// Execute the `env` command.
///
/// Writes each environment variable as `KEY=value\n` to `writer`.
/// Keys are emitted in sorted order because `PreviewState.env` is a `BTreeMap`.
/// If the environment is empty, nothing is written.
pub fn execute(state: &PreviewState, writer: &mut impl Write) -> Result<(), ReplError> {
    for (key, value) in &state.env {
        writeln!(writer, "{key}={value}").map_err(|e| ReplError::InvalidArguments {
            command: "env".to_string(),
            message: e.to_string(),
        })?;
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
}
