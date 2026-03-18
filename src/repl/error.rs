// Error types for the REPL command layer.
//
// `ReplError` is the single error type returned by all command handler
// functions. Each variant carries enough context for the REPL loop to
// print a meaningful, user-friendly message without exposing internal details.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur while executing a REPL command.
///
/// Each variant maps to a distinct failure mode that the REPL loop displays
/// as a compact, terminal-friendly message. Command handlers return
/// `Result<(), ReplError>` so the REPL can format errors consistently.
#[derive(Debug, Error, PartialEq)]
pub enum ReplError {
    /// The requested path does not exist in the virtual filesystem.
    #[error("no such file or directory: {path}")]
    PathNotFound { path: PathBuf },

    /// The path exists but is a file, not a directory (e.g. `cd` into a file).
    #[error("not a directory: {path}")]
    NotADirectory { path: PathBuf },

    /// The path exists but is a directory, not a file (e.g. `cat` on a directory).
    #[error("is a directory: {path}")]
    NotAFile { path: PathBuf },

    /// The command was called with invalid or missing arguments.
    #[error("{command}: {message}")]
    InvalidArguments { command: String, message: String },

    /// The input did not match any known command.
    #[error("unknown command: '{input}'. Type 'help' for available commands.")]
    UnknownCommand { input: String },
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- PathNotFound ---

    #[test]
    fn path_not_found_display_contains_path() {
        let err = ReplError::PathNotFound {
            path: PathBuf::from("/app/missing.txt"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/app/missing.txt"), "got: {msg}");
    }

    #[test]
    fn path_not_found_equality() {
        let a = ReplError::PathNotFound {
            path: PathBuf::from("/a"),
        };
        let b = ReplError::PathNotFound {
            path: PathBuf::from("/a"),
        };
        assert_eq!(a, b);
    }

    // --- NotADirectory ---

    #[test]
    fn not_a_directory_display_contains_path() {
        let err = ReplError::NotADirectory {
            path: PathBuf::from("/app/file.txt"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/app/file.txt"), "got: {msg}");
        assert!(
            msg.contains("not a directory") || msg.contains("directory"),
            "got: {msg}"
        );
    }

    // --- NotAFile ---

    #[test]
    fn not_a_file_display_contains_path() {
        let err = ReplError::NotAFile {
            path: PathBuf::from("/app"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/app"), "got: {msg}");
    }

    // --- InvalidArguments ---

    #[test]
    fn invalid_arguments_display_contains_command_and_message() {
        let err = ReplError::InvalidArguments {
            command: "cat".to_string(),
            message: "requires a file path".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("cat"), "got: {msg}");
        assert!(msg.contains("requires a file path"), "got: {msg}");
    }

    // --- UnknownCommand ---

    #[test]
    fn unknown_command_display_contains_input() {
        let err = ReplError::UnknownCommand {
            input: "frobnicate".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("frobnicate"), "got: {msg}");
        assert!(msg.contains("help"), "should mention 'help', got: {msg}");
    }

    // --- Debug (derives) ---

    #[test]
    fn repl_error_is_debug() {
        let err = ReplError::PathNotFound {
            path: PathBuf::from("/x"),
        };
        let debug = format!("{err:?}");
        assert!(!debug.is_empty());
    }
}
