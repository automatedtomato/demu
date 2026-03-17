// RUN instruction stub for the simulation engine.
//
// Real shell command execution is intentionally outside scope for v0.1.
// This module records the command in history and emits a warning so the
// user understands the command was not executed.

use crate::model::{state::{LayerSummary, PreviewState}, warning::Warning};

/// Handle a `RUN` instruction by recording it as unmodeled.
///
/// Pushes a `Warning::UnmodeledRunCommand` to `state.warnings` and
/// returns a `LayerSummary` with no filesystem or environment changes.
/// The command text is preserved so it appears in `:history`.
pub(crate) fn handle_run(
    state: &mut PreviewState,
    command: &str,
    _line: usize,
) -> LayerSummary {
    state.warnings.push(Warning::UnmodeledRunCommand {
        command: command.to_string(),
    });

    LayerSummary {
        instruction_type: "RUN".to_string(),
        files_changed: vec![],
        env_changed: vec![],
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::model::{state::PreviewState, warning::Warning};

    // ── test: handle_run emits UnmodeledRunCommand warning ────────────────

    #[test]
    fn run_emits_unmodeled_warning() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "echo hello", 1);

        let has_warning = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { command } if command == "echo hello"));
        assert!(has_warning, "expected UnmodeledRunCommand warning");
    }

    // ── test: layer summary has instruction_type == "RUN" ─────────────────

    #[test]
    fn run_layer_summary_has_correct_type() {
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "make build", 1);
        assert_eq!(layer.instruction_type, "RUN");
    }

    // ── test: layer summary has no files_changed ──────────────────────────

    #[test]
    fn run_layer_summary_has_no_files_changed() {
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "touch /tmp/x", 1);
        assert!(
            layer.files_changed.is_empty(),
            "RUN stub must not record file changes"
        );
    }

    // ── test: handle_run does not mutate fs or env ────────────────────────

    #[test]
    fn run_does_not_mutate_fs_or_env() {
        let mut state = PreviewState::default();
        let fs_before = state.fs.iter().count();
        let env_before = state.env.len();

        handle_run(&mut state, "apt-get install -y curl", 1);

        assert_eq!(
            state.fs.iter().count(),
            fs_before,
            "fs must not change after RUN stub"
        );
        assert_eq!(
            state.env.len(),
            env_before,
            "env must not change after RUN stub"
        );
    }
}
