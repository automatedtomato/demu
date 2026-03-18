// `:layers` command — display a layer-by-layer summary of changes.
//
// Reads `PreviewState::layers` (a `Vec<LayerSummary>`) and prints a
// numbered table. Each row shows the instruction type, the file count,
// and the env-var count. This is a pure read command; it never mutates
// `PreviewState`.

use std::io::Write;

use crate::model::state::PreviewState;
use crate::repl::error::ReplError;
use crate::repl::sanitize::sanitize_for_terminal;

/// Execute the `:layers` command.
///
/// Prints a numbered summary table of every recorded layer. Each line has the
/// format:
///
/// ```text
/// Layer N  TYPE        F file(s) changed, E env var(s) changed
/// ```
///
/// Where `TYPE` is left-padded to 11 characters, `F` is `files_changed.len()`,
/// and `E` is `env_changed.len()`.
///
/// When `state.layers` is empty the message `"No layers recorded."` is printed
/// instead.
///
/// All output goes to `writer`; I/O errors are mapped to
/// [`ReplError::InvalidArguments`].
pub fn execute(state: &PreviewState, writer: &mut impl Write) -> Result<(), ReplError> {
    // Map I/O errors into a ReplError so callers have a uniform error type.
    let io_err = |e: std::io::Error| ReplError::InvalidArguments {
        command: ":layers".to_string(),
        message: e.to_string(),
    };

    if state.layers.is_empty() {
        return writeln!(writer, "No layers recorded.").map_err(io_err);
    }

    // Compute the digit width of the largest layer index for right-aligned
    // numbering. This keeps columns aligned even when there are 10+ layers.
    let num_width = state.layers.len().to_string().len();

    for (i, layer) in state.layers.iter().enumerate() {
        // Layer index is 1-based, matching Docker's own layer numbering.
        let layer_num = i + 1;
        let file_count = layer.files_changed.len();
        let env_count = layer.env_changed.len();

        // Sanitize instruction_type defensively: it is engine-generated today,
        // but if future code ever populates it from raw Dockerfile text we do
        // not want to introduce a terminal escape injection path.
        let safe_type = sanitize_for_terminal(&layer.instruction_type);

        // Instruction type is left-padded to 11 chars for column alignment.
        writeln!(
            writer,
            "Layer {:>width$}  {:<11}  {} file(s) changed, {} env var(s) changed",
            layer_num,
            safe_type,
            file_count,
            env_count,
            width = num_width,
        )
        .map_err(io_err)?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::state::{LayerSummary, PreviewState};
    use std::path::PathBuf;

    /// Run `execute` and return the captured output as a `String`.
    fn run(state: &PreviewState) -> String {
        let mut buf = Vec::new();
        execute(state, &mut buf).expect(":layers should not fail");
        String::from_utf8(buf).expect("output must be utf-8")
    }

    // --- Empty state ---

    #[test]
    fn empty_layers_prints_no_layers_message() {
        let state = PreviewState::default();
        let out = run(&state);
        assert_eq!(out.trim(), "No layers recorded.");
    }

    // --- Single layer with files only ---

    #[test]
    fn single_layer_files_only_shows_file_count_and_zero_env() {
        let mut state = PreviewState::default();
        state.layers.push(LayerSummary {
            instruction_type: "COPY".to_string(),
            files_changed: vec![PathBuf::from("/app/main.rs"), PathBuf::from("/app/lib.rs")],
            env_changed: vec![],
        });
        let out = run(&state);
        // Must show layer number and file count.
        assert!(out.contains("Layer 1"), "got: {out}");
        assert!(out.contains("COPY"), "got: {out}");
        assert!(out.contains("2 file(s)"), "got: {out}");
        // Zero env vars must also be represented.
        assert!(out.contains("0 env var(s)"), "got: {out}");
    }

    // --- Single layer with env vars ---

    #[test]
    fn single_layer_with_env_vars_shows_env_count() {
        let mut state = PreviewState::default();
        state.layers.push(LayerSummary {
            instruction_type: "ENV".to_string(),
            files_changed: vec![],
            env_changed: vec![("PATH".to_string(), "/usr/bin".to_string())],
        });
        let out = run(&state);
        assert!(out.contains("Layer 1"), "got: {out}");
        assert!(out.contains("ENV"), "got: {out}");
        assert!(out.contains("0 file(s)"), "got: {out}");
        assert!(out.contains("1 env var(s)"), "got: {out}");
    }

    // --- Multiple layers are numbered sequentially ---

    #[test]
    fn multiple_layers_are_numbered_in_order() {
        let mut state = PreviewState::default();
        state.layers.push(LayerSummary {
            instruction_type: "COPY".to_string(),
            files_changed: vec![PathBuf::from("/app/main.rs")],
            env_changed: vec![],
        });
        state.layers.push(LayerSummary {
            instruction_type: "RUN".to_string(),
            files_changed: vec![],
            env_changed: vec![("DEBIAN_FRONTEND".to_string(), "noninteractive".to_string())],
        });
        state.layers.push(LayerSummary {
            instruction_type: "WORKDIR".to_string(),
            files_changed: vec![],
            env_changed: vec![],
        });
        let out = run(&state);

        // All three layers must appear in order.
        let pos1 = out.find("Layer 1").expect("Layer 1 must appear");
        let pos2 = out.find("Layer 2").expect("Layer 2 must appear");
        let pos3 = out.find("Layer 3").expect("Layer 3 must appear");
        assert!(pos1 < pos2, "Layer 1 must come before Layer 2");
        assert!(pos2 < pos3, "Layer 2 must come before Layer 3");

        // Instruction types must appear on the correct lines.
        assert!(out.contains("COPY"), "got: {out}");
        assert!(out.contains("RUN"), "got: {out}");
        assert!(out.contains("WORKDIR"), "got: {out}");
    }

    // --- Layer with zero files and zero env vars ---

    #[test]
    fn layer_with_zero_files_and_zero_env_shows_both_zeros() {
        let mut state = PreviewState::default();
        state.layers.push(LayerSummary {
            instruction_type: "FROM".to_string(),
            files_changed: vec![],
            env_changed: vec![],
        });
        let out = run(&state);
        assert!(out.contains("0 file(s)"), "got: {out}");
        assert!(out.contains("0 env var(s)"), "got: {out}");
    }

    // --- execute returns Ok ---

    #[test]
    fn execute_returns_ok_for_empty_state() {
        let state = PreviewState::default();
        let mut buf = Vec::new();
        assert!(execute(&state, &mut buf).is_ok());
    }

    #[test]
    fn execute_returns_ok_for_non_empty_state() {
        let mut state = PreviewState::default();
        state.layers.push(LayerSummary {
            instruction_type: "RUN".to_string(),
            files_changed: vec![],
            env_changed: vec![],
        });
        let mut buf = Vec::new();
        assert!(execute(&state, &mut buf).is_ok());
    }
}
