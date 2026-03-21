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

/// Errors produced by the Compose YAML parser.
///
/// Variants distinguish structural problems (missing keys, invalid service
/// definitions) from low-level YAML syntax errors so callers can surface
/// meaningful messages to the user.
#[derive(Debug, Error)]
pub enum ComposeParseError {
    /// The YAML could not be parsed at all (syntax error or type mismatch).
    #[error("invalid YAML: {message}")]
    InvalidYaml { message: String },

    /// The top-level `services:` key is absent from the Compose file.
    #[error("compose file is missing the 'services' key")]
    MissingServicesKey,

    /// A service definition is structurally invalid.
    #[error("invalid service '{name}': {message}")]
    InvalidService { name: String, message: String },
}
