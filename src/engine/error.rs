use std::path::PathBuf;
use thiserror::Error;

/// Errors that the engine cannot recover from by emitting a warning.
///
/// Recoverable conditions (e.g. missing COPY source) become `Warning`s in
/// `PreviewState`, not `EngineError`s.
#[derive(Debug, Error)]
pub enum EngineError {
    /// An I/O operation on a host file failed for a reason other than
    /// "file not found" (which is handled as a `Warning::MissingCopySource`).
    #[error("I/O error reading '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
