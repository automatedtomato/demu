// RUN instruction stub for the simulation engine.
//
// Real shell command execution is intentionally outside scope for v0.1.
// This module records the command in history and emits a warning so the
// user understands the command was not executed.
//
// `&&`-chain and `;`-separated commands are split into individual sub-commands,
// each recorded as a separate `UnmodeledRunCommand` warning. This gives the
// user a per-sub-command trace without silently collapsing multi-step RUN
// instructions into one opaque entry.

use std::path::PathBuf;

use crate::model::{
    state::{LayerSummary, PreviewState},
    warning::Warning,
};

// ─── Internal types ───────────────────────────────────────────────────────────

/// Accumulated changes produced by a single sub-command dispatch.
///
/// All fields start empty. Future issues (#20/#21) will populate them when
/// specific commands (e.g. `apt-get install`, `mkdir`) are modeled.
struct SubCommandResult {
    /// Filesystem paths created, modified, or removed by the sub-command.
    files_changed: Vec<PathBuf>,
    /// Environment variable assignments produced by the sub-command.
    env_changed: Vec<(String, String)>,
}

impl SubCommandResult {
    /// Construct an empty result (no filesystem or environment changes).
    fn empty() -> Self {
        Self {
            files_changed: vec![],
            env_changed: vec![],
        }
    }
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Split a raw shell command string into individual sub-commands.
///
/// Handles `&&`-chains and `;`-separated sequences. Each segment is trimmed
/// and empty segments are discarded.
///
/// **Limitation:** shell quoting is not modeled. A `&&` or `;` inside a
/// quoted string (e.g. `echo "a && b"`) will be treated as a delimiter.
/// This is an in-scope approximation for v0.1.
fn split_commands(raw: &str) -> Vec<&str> {
    // Two-pass split: first on `&&`, then on `;` within each segment.
    // All returned slices borrow from `raw` — no intermediate allocations.
    // `||` and other shell operators are intentionally not split and
    // pass through as part of a single sub-command token.
    raw.split("&&")
        .flat_map(|segment| segment.split(';'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Dispatch a single sub-command against the current simulation state.
///
/// Pushes an `UnmodeledRunCommand` warning for every sub-command so the user
/// can see exactly which shell steps were encountered but not modeled.
///
/// Future issues (#20 `apt-get`, #21 filesystem mutations) will add match
/// arms here to simulate specific commands before falling through to the
/// unmodeled case.
fn dispatch_sub_command(state: &mut PreviewState, sub_cmd: &str) -> SubCommandResult {
    // Record the sub-command as unmodeled so the user sees it in warnings.
    state.warnings.push(Warning::UnmodeledRunCommand {
        command: sub_cmd.to_string(),
    });

    // No filesystem or environment changes for unmodeled commands.
    SubCommandResult::empty()
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Handle a `RUN` instruction by splitting it into sub-commands and recording
/// each one as unmodeled.
///
/// `&&`-chains and `;`-separated commands are each dispatched individually so
/// the user gets a per-sub-command warning entry rather than a single opaque
/// blob. All resulting `files_changed` and `env_changed` entries are merged
/// into a single `LayerSummary` (matching Docker's one-layer-per-RUN model).
///
/// No real shell commands are executed. Every sub-command is preserved in
/// `state.warnings` so it appears in `:history`.
pub(crate) fn handle_run(state: &mut PreviewState, command: &str, _line: usize) -> LayerSummary {
    let sub_commands = split_commands(command);

    // Dispatch each sub-command and merge the results.
    let mut all_files: Vec<PathBuf> = Vec::new();
    let mut all_env: Vec<(String, String)> = Vec::new();

    for sub_cmd in sub_commands {
        let result = dispatch_sub_command(state, sub_cmd);
        all_files.extend(result.files_changed);
        all_env.extend(result.env_changed);
    }

    LayerSummary {
        instruction_type: "RUN".to_string(),
        files_changed: all_files,
        env_changed: all_env,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::model::{state::PreviewState, warning::Warning};

    // ── Pre-existing tests (must remain unchanged and green) ──────────────

    #[test]
    fn run_emits_unmodeled_warning() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "echo hello", 1);

        let has_warning = state.warnings.iter().any(
            |w| matches!(w, Warning::UnmodeledRunCommand { command } if command == "echo hello"),
        );
        assert!(has_warning, "expected UnmodeledRunCommand warning");
    }

    #[test]
    fn run_layer_summary_has_correct_type() {
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "make build", 1);
        assert_eq!(layer.instruction_type, "RUN");
    }

    #[test]
    fn run_layer_summary_has_no_files_changed() {
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "touch /tmp/x", 1);
        assert!(
            layer.files_changed.is_empty(),
            "RUN stub must not record file changes"
        );
    }

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

    // ── split_commands unit tests ─────────────────────────────────────────

    #[test]
    fn split_single_command() {
        assert_eq!(split_commands("echo hello"), vec!["echo hello"]);
    }

    #[test]
    fn split_and_chain() {
        assert_eq!(
            split_commands("apt-get update && apt-get install -y curl"),
            vec!["apt-get update", "apt-get install -y curl"]
        );
    }

    #[test]
    fn split_semicolon_chain() {
        assert_eq!(split_commands("echo a; echo b"), vec!["echo a", "echo b"]);
    }

    #[test]
    fn split_mixed_delimiters() {
        assert_eq!(
            split_commands("echo a && echo b; echo c"),
            vec!["echo a", "echo b", "echo c"]
        );
    }

    #[test]
    fn split_trims_whitespace() {
        assert_eq!(
            split_commands("  echo a  &&  echo b  "),
            vec!["echo a", "echo b"]
        );
    }

    #[test]
    fn split_skips_empty_segments() {
        // Two consecutive `&&` produce an empty segment between them.
        assert_eq!(
            split_commands("echo a && && echo b"),
            vec!["echo a", "echo b"]
        );
    }

    #[test]
    fn split_only_delimiters() {
        // A string composed only of delimiters and whitespace yields no commands.
        assert_eq!(split_commands("&& ; &&"), Vec::<&str>::new());
    }

    #[test]
    fn split_empty_string() {
        assert_eq!(split_commands(""), Vec::<&str>::new());
    }

    #[test]
    fn split_trailing_and_chain() {
        // A trailing `&&` produces no extra empty element.
        assert_eq!(split_commands("echo a &&"), vec!["echo a"]);
    }

    #[test]
    fn split_does_not_split_on_or_operator() {
        // `||` is not a recognized delimiter and must be preserved intact.
        assert_eq!(split_commands("cmd1 || fallback"), vec!["cmd1 || fallback"]);
    }

    // ── handle_run integration tests ──────────────────────────────────────

    #[test]
    fn run_and_chain_emits_per_subcommand_warnings() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "apt-get update && apt-get install -y curl", 1);

        assert_eq!(
            state.warnings.len(),
            2,
            "expected one warning per sub-command, got {}",
            state.warnings.len()
        );

        // First warning must reference the first sub-command.
        assert!(
            matches!(
                &state.warnings[0],
                Warning::UnmodeledRunCommand { command } if command == "apt-get update"
            ),
            "first warning should be for 'apt-get update', got: {:?}",
            state.warnings[0]
        );

        // Second warning must reference the second sub-command.
        assert!(
            matches!(
                &state.warnings[1],
                Warning::UnmodeledRunCommand { command } if command == "apt-get install -y curl"
            ),
            "second warning should be for 'apt-get install -y curl', got: {:?}",
            state.warnings[1]
        );
    }

    #[test]
    fn run_semicolon_chain_emits_per_subcommand_warnings() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "echo a; echo b; echo c", 1);

        assert_eq!(
            state.warnings.len(),
            3,
            "expected 3 warnings for 3 semicolon-separated sub-commands"
        );
        // Verify each warning carries the correct command text, not just the count.
        assert!(
            matches!(&state.warnings[0], Warning::UnmodeledRunCommand { command } if command == "echo a"),
            "first warning should be for 'echo a'"
        );
        assert!(
            matches!(&state.warnings[1], Warning::UnmodeledRunCommand { command } if command == "echo b"),
            "second warning should be for 'echo b'"
        );
        assert!(
            matches!(&state.warnings[2], Warning::UnmodeledRunCommand { command } if command == "echo c"),
            "third warning should be for 'echo c'"
        );
    }

    #[test]
    fn run_chain_returns_single_layer_summary() {
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "a && b && c", 1);

        assert_eq!(layer.instruction_type, "RUN");
        assert!(
            layer.files_changed.is_empty(),
            "unmodeled commands must not produce file changes"
        );
        assert!(
            layer.env_changed.is_empty(),
            "unmodeled commands must not produce env changes"
        );
    }

    #[test]
    fn run_or_operator_preserved_as_single_warning() {
        // `||` is not a delimiter — the entire token must appear in one warning.
        let mut state = PreviewState::default();
        handle_run(&mut state, "cmd1 || fallback", 1);

        assert_eq!(
            state.warnings.len(),
            1,
            "|| must not split into two warnings"
        );
        assert!(
            matches!(
                &state.warnings[0],
                Warning::UnmodeledRunCommand { command } if command == "cmd1 || fallback"
            ),
            "full '||' token must be preserved in the single warning"
        );
    }

    #[test]
    fn run_empty_command_emits_no_warnings() {
        // An empty RUN command string produces zero warnings and a valid LayerSummary.
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "", 1);

        assert!(
            state.warnings.is_empty(),
            "empty command must emit no warnings"
        );
        assert_eq!(layer.instruction_type, "RUN");
        assert!(layer.files_changed.is_empty());
        assert!(layer.env_changed.is_empty());
    }

    #[test]
    fn run_single_command_still_works() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "echo hello", 1);

        assert_eq!(
            state.warnings.len(),
            1,
            "single command should produce exactly one warning"
        );
        assert!(
            matches!(
                &state.warnings[0],
                Warning::UnmodeledRunCommand { command } if command == "echo hello"
            ),
            "warning command text must match the input"
        );
    }
}
