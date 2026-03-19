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
    ///
    /// Note: `path` stores the raw, unsanitized value. Callers must apply
    /// `sanitize_for_terminal` before printing the display string to a terminal.
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

/// Returns a closure that maps a [`std::io::Error`] into
/// [`ReplError::InvalidArguments`] for the given command name.
///
/// This is a shared helper so that command handlers do not each have to
/// define the same 3-line inline closure. Use it wherever a `writeln!` or
/// similar I/O operation needs to map its error into `ReplError`:
///
/// ```ignore
/// use crate::repl::error::io_err_mapper;
///
/// let io_err = io_err_mapper("apt");
/// writeln!(writer, "...").map_err(&io_err)?;
/// ```
///
/// The returned closure is `'static` — it owns an allocated `String` copy of
/// `command` and has no lifetime dependency on the original `&str`.
pub fn io_err_mapper(command: &str) -> impl Fn(std::io::Error) -> ReplError {
    let command = command.to_owned();
    move |e| ReplError::InvalidArguments {
        command: command.clone(),
        message: e.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io;
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

    // --- io_err_mapper ---

    /// `io_err_mapper` must produce `ReplError::InvalidArguments` with the
    /// exact command name and the I/O error message string.
    #[test]
    fn io_err_mapper_produces_invalid_arguments_with_command_name() {
        let mapper = io_err_mapper("test-cmd");
        let raw_err = io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe");
        let err = mapper(raw_err);
        assert_eq!(
            err,
            ReplError::InvalidArguments {
                command: "test-cmd".to_string(),
                message: "broken pipe".to_string(),
            },
            "io_err_mapper must wrap the io::Error into InvalidArguments; got: {err:?}"
        );
    }

    /// `io_err_mapper` must be callable multiple times (the closure is `Fn`, not `FnOnce`).
    #[test]
    fn io_err_mapper_is_callable_multiple_times() {
        let mapper = io_err_mapper("multi");
        let e1 = mapper(io::Error::new(io::ErrorKind::Other, "first"));
        let e2 = mapper(io::Error::new(io::ErrorKind::Other, "second"));
        assert_eq!(
            e1,
            ReplError::InvalidArguments {
                command: "multi".to_string(),
                message: "first".to_string(),
            }
        );
        assert_eq!(
            e2,
            ReplError::InvalidArguments {
                command: "multi".to_string(),
                message: "second".to_string(),
            }
        );
    }

    /// `io_err_mapper` must preserve the command name exactly (no truncation, no transforms).
    #[test]
    fn io_err_mapper_preserves_command_name_with_colon_prefix() {
        let err = io_err_mapper(":reload")(io::Error::new(io::ErrorKind::Other, "oops"));
        assert_eq!(
            err,
            ReplError::InvalidArguments {
                command: ":reload".to_string(),
                message: "oops".to_string(),
            },
            "colon-prefixed command name must be preserved; got: {err:?}"
        );
    }
}
