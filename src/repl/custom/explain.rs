// Handler for the `:explain <path>` REPL command.
//
// Resolves the input path against the virtual filesystem and writes a
// multi-line provenance report to the output writer. Returns
// `ReplError::InvalidArguments` when no path is given, and
// `ReplError::PathNotFound` when the resolved path is absent from the
// virtual filesystem.

use std::io::Write;

use crate::explain;
use crate::explain::ExplainError;
use crate::model::state::PreviewState;
use crate::repl::{error::ReplError, path::resolve_path};

/// Execute `:explain <path>`: resolve the path and display its provenance.
///
/// An empty `path` argument results in `ReplError::InvalidArguments` so the
/// REPL can surface a usage hint rather than a confusing path-not-found error.
pub fn execute(state: &PreviewState, path: &str, writer: &mut impl Write) -> Result<(), ReplError> {
    // Guard: `:explain` with no argument is a usage error.
    if path.is_empty() {
        return Err(ReplError::InvalidArguments {
            command: ":explain".to_string(),
            message: "requires a path argument".to_string(),
        });
    }

    // Resolve the input path relative to the current working directory.
    let resolved = resolve_path(&state.cwd, path);

    // Delegate provenance lookup to the explain module.
    let report = explain::explain_path(state, &resolved).map_err(|e| match e {
        ExplainError::PathNotFound { path } => ReplError::PathNotFound { path },
    })?;

    // Write the report followed by a newline so it is cleanly separated from
    // the next REPL prompt.
    writeln!(writer, "{report}").map_err(|e| ReplError::InvalidArguments {
        command: ":explain".to_string(),
        message: e.to_string(),
    })
}
