// `pwd` command — print working directory.
//
// Reads the current working directory from `PreviewState` and writes it
// followed by a newline. This is a read-only, infallible command.

use std::io::Write;

use crate::model::state::PreviewState;
use crate::repl::error::ReplError;

/// Execute the `pwd` command.
///
/// Writes the current working directory from `state.cwd` to `writer`,
/// followed by a newline. The command never fails.
pub fn execute(state: &PreviewState, writer: &mut impl Write) -> Result<(), ReplError> {
    // Display the cwd as a string; PathBuf::display() is safe on all platforms.
    writeln!(writer, "{}", state.cwd.display()).map_err(|e| ReplError::Io {
        command: "pwd".to_string(),
        message: e.to_string(),
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::state::PreviewState;
    use std::path::PathBuf;

    fn output_for(state: &PreviewState) -> String {
        let mut buf = Vec::new();
        execute(state, &mut buf).expect("pwd should not fail");
        String::from_utf8(buf).expect("output must be utf-8")
    }

    // --- Happy path ---

    #[test]
    fn pwd_prints_root_cwd() {
        let state = PreviewState::default(); // cwd = /
        let out = output_for(&state);
        assert_eq!(out.trim(), "/");
    }

    #[test]
    fn pwd_prints_app_cwd() {
        let mut state = PreviewState::default();
        state.cwd = PathBuf::from("/app");
        let out = output_for(&state);
        assert_eq!(out.trim(), "/app");
    }

    #[test]
    fn pwd_prints_nested_cwd() {
        let mut state = PreviewState::default();
        state.cwd = PathBuf::from("/app/src");
        let out = output_for(&state);
        assert_eq!(out.trim(), "/app/src");
    }

    #[test]
    fn pwd_output_ends_with_newline() {
        let state = PreviewState::default();
        let mut buf = Vec::new();
        execute(&state, &mut buf).expect("pwd should not fail");
        let out = String::from_utf8(buf).expect("utf-8");
        assert!(out.ends_with('\n'), "output must end with newline");
    }

    #[test]
    fn pwd_returns_ok() {
        let state = PreviewState::default();
        let mut buf = Vec::new();
        assert!(execute(&state, &mut buf).is_ok());
    }
}
