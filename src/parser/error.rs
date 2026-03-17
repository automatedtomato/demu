use thiserror::Error;

/// Errors produced by the Dockerfile parser.
///
/// Each variant carries a 1-based line number so the caller can surface
/// the exact location of the problem to the user.
#[derive(Debug, Error)]
pub enum ParseError {
    /// A Dockerfile instruction was malformed or had missing required arguments.
    #[error("line {line}: {message}")]
    InvalidInstruction { line: usize, message: String },
}
