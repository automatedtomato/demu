// Interactive REPL shell loop for demu.
//
// `run_repl` is the single public entry point. It owns the rustyline editor,
// reads lines from the terminal, dispatches each parsed command to the
// appropriate handler, and manages the session lifecycle (Ctrl-C, Ctrl-D,
// and the `exit` command).

pub mod commands;
pub mod config;
pub mod custom;
pub mod error;
pub mod parse;
pub mod path;

/// Placeholder struct kept for backward-compatibility with integration tests
/// that were written against the initial module scaffold.
///
/// The real REPL entry point is [`run_repl`].
pub struct Repl;

use std::io::{self, Write};

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::model::state::PreviewState;
use crate::output::sanitize::sanitize_for_terminal;
use crate::repl::commands::{apt, cat, cd, env_cmd, find, help, ls, pip, pwd, which};
use crate::repl::config::ReplConfig;
use crate::repl::custom::{explain, history, installed, layers, reload};
use crate::repl::error::ReplError;
use crate::repl::parse::{parse_input, ParsedCommand};

/// Run the interactive REPL until the user exits.
///
/// The REPL:
/// - Displays a dynamic prompt showing the current working directory.
/// - Parses each input line into a [`ParsedCommand`].
/// - Handles `:reload` inline before calling `dispatch` (because reload needs
///   access to `config` and both stdout and stderr writers simultaneously).
/// - Dispatches all other commands to [`dispatch`].
/// - Prints errors from handlers as compact terminal messages (non-fatal).
/// - Exits gracefully on Ctrl-D, `exit`, or `quit`.
/// - Continues on Ctrl-C (prints a hint on how to exit).
pub fn run_repl(state: &mut PreviewState, config: &ReplConfig) -> anyhow::Result<()> {
    let mut editor = DefaultEditor::new()?;
    let stdout = io::stdout();

    loop {
        // Build the prompt from the current working directory.
        // Sanitize the cwd before embedding it in the prompt — WORKDIR
        // instructions are user-controlled and could contain ANSI escape bytes.
        let safe_cwd = sanitize_for_terminal(&state.cwd.display().to_string());
        let prompt = format!("demu:{safe_cwd}$ ");

        match editor.readline(&prompt) {
            Ok(line) => {
                // Add non-empty lines to history for up-arrow recall.
                if !line.trim().is_empty() {
                    let _ = editor.add_history_entry(&line);
                }

                let cmd = parse_input(&line);
                let mut out = stdout.lock();

                // `:reload` is handled before dispatch because it requires
                // both a stdout writer and a stderr writer simultaneously, and
                // it also needs the session-level `config`. It is intentionally
                // NOT part of `dispatch` — see the comment in `dispatch` below.
                if cmd == ParsedCommand::Reload {
                    let mut err_out = io::stderr();
                    if let Err(e) = reload::execute(state, config, &mut out, &mut err_out) {
                        let safe = sanitize_for_terminal(&e.to_string());
                        eprintln!("{safe}");
                    }
                    continue;
                }

                match dispatch(state, cmd, &mut out) {
                    // `exit` / `quit` — terminate the loop cleanly.
                    Ok(false) => break,
                    Ok(true) => {}
                    Err(ReplError::UnknownCommand { ref input }) => {
                        let safe = sanitize_for_terminal(input);
                        eprintln!("unknown command: '{safe}'. Type 'help' for available commands.");
                    }
                    Err(other) => {
                        // Sanitize the error string before printing — error messages
                        // include user-supplied path components which may contain
                        // ANSI escape sequences (C0 control chars, C1 range).
                        let safe = sanitize_for_terminal(&other.to_string());
                        eprintln!("{safe}");
                    }
                }
            }
            // Ctrl-C — cancel the current line, continue the loop.
            Err(ReadlineError::Interrupted) => {
                eprintln!("(to exit, type 'exit' or press Ctrl-D)");
            }
            // Ctrl-D (EOF) — exit gracefully.
            Err(ReadlineError::Eof) => {
                break;
            }
            // Other I/O errors — surface and abort.
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    Ok(())
}

/// Dispatch a parsed command to the appropriate handler.
///
/// All handlers write their output to `writer` rather than to stdout, which
/// makes this function fully testable without capturing stdout.
///
/// Returns `Ok(true)` normally or `Ok(false)` when the command signals exit.
/// The caller is responsible for exiting the loop on `Exit`.
///
/// Note: `ParsedCommand::Reload` is intentionally NOT handled here. It is
/// intercepted in `run_repl` before this function is called, because reload
/// needs access to both stdout and stderr writers as well as the session-level
/// `ReplConfig`. This design avoids threading config through dispatch.
pub fn dispatch(
    state: &mut PreviewState,
    cmd: ParsedCommand,
    writer: &mut impl Write,
) -> Result<bool, ReplError> {
    match cmd {
        ParsedCommand::Pwd => {
            pwd::execute(state, writer)?;
        }
        ParsedCommand::Env => {
            env_cmd::execute(state, writer)?;
        }
        ParsedCommand::Help => {
            help::execute(state, writer)?;
        }
        ParsedCommand::Ls { path, long } => {
            ls::execute(state, path.as_deref(), long, writer)?;
        }
        ParsedCommand::Cat { path } => {
            cat::execute(state, &path, writer)?;
        }
        ParsedCommand::Cd { path } => {
            cd::execute(state, &path, writer)?;
        }
        ParsedCommand::Find { path, name_pattern } => {
            find::execute(state, &path, name_pattern.as_deref(), writer)?;
        }
        ParsedCommand::Layers => {
            layers::execute(state, writer)?;
        }
        ParsedCommand::History => {
            history::execute(state, writer)?;
        }
        ParsedCommand::Installed => {
            installed::execute(state, writer)?;
        }
        ParsedCommand::AptList { installed } => {
            apt::execute(state, installed, writer)?;
        }
        ParsedCommand::PipList => {
            pip::execute(state, writer)?;
        }
        ParsedCommand::Which { cmd } => {
            which::execute(state, &cmd, writer)?;
        }
        ParsedCommand::Explain { path } => {
            explain::execute(state, &path, writer)?;
        }
        ParsedCommand::Exit => {
            return Ok(false);
        }
        ParsedCommand::Empty => {
            // Nothing to do — just re-prompt.
        }
        ParsedCommand::Unknown { input } => {
            return Err(ReplError::UnknownCommand { input });
        }
        // Reload is intercepted in `run_repl` before `dispatch` is called.
        // Reaching this arm indicates a programming error in the REPL loop.
        ParsedCommand::Reload => {
            unreachable!(":reload must be handled in run_repl before dispatch is called");
        }
    }

    Ok(true)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::fs::{DirNode, FileNode, FsNode, VirtualFs};
    use crate::model::provenance::{Provenance, ProvenanceSource};
    use crate::model::state::PreviewState;
    use std::path::PathBuf;

    fn make_provenance() -> Provenance {
        Provenance::new(ProvenanceSource::Workdir)
    }

    fn file_node(content: &[u8]) -> FsNode {
        FsNode::File(FileNode {
            content: content.to_vec(),
            provenance: make_provenance(),
            permissions: None,
        })
    }

    fn dir_node() -> FsNode {
        FsNode::Directory(DirNode {
            provenance: make_provenance(),
            permissions: None,
        })
    }

    fn dispatch_str(state: &mut PreviewState, input: &str) -> Result<(bool, String), ReplError> {
        let cmd = parse_input(input);
        let mut buf = Vec::new();
        let cont = dispatch(state, cmd, &mut buf)?;
        Ok((cont, String::from_utf8(buf).expect("utf-8")))
    }

    fn state_with_files() -> PreviewState {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        fs.insert(PathBuf::from("/app/main.rs"), file_node(b"fn main() {}"));
        fs.insert(PathBuf::from("/app/lib.rs"), file_node(b"// lib"));
        let mut state = PreviewState::default();
        state.fs = fs;
        state.env.insert("PATH".to_string(), "/usr/bin".to_string());
        state.env.insert("HOME".to_string(), "/root".to_string());
        state
    }

    // --- pwd dispatch ---

    #[test]
    fn dispatch_pwd_prints_cwd() {
        let mut state = PreviewState::default();
        let (cont, out) = dispatch_str(&mut state, "pwd").expect("should succeed");
        assert!(cont);
        assert_eq!(out.trim(), "/");
    }

    // --- env dispatch ---

    #[test]
    fn dispatch_env_prints_sorted_env_vars() {
        let mut state = state_with_files();
        let (cont, out) = dispatch_str(&mut state, "env").expect("should succeed");
        assert!(cont);
        assert!(out.contains("PATH=/usr/bin"), "got: {out}");
        assert!(out.contains("HOME=/root"), "got: {out}");
        // HOME < PATH alphabetically.
        let home_pos = out.find("HOME").expect("HOME must appear");
        let path_pos = out.find("PATH").expect("PATH must appear");
        assert!(home_pos < path_pos, "HOME must come before PATH");
    }

    // --- ls dispatch ---

    #[test]
    fn dispatch_ls_lists_app_contents() {
        let mut state = state_with_files();
        state.cwd = PathBuf::from("/app");
        let (cont, out) = dispatch_str(&mut state, "ls").expect("should succeed");
        assert!(cont);
        assert!(out.contains("main.rs"), "got: {out}");
        assert!(out.contains("lib.rs"), "got: {out}");
    }

    // --- cat dispatch ---

    #[test]
    fn dispatch_cat_prints_file_content() {
        let mut state = state_with_files();
        let (cont, out) = dispatch_str(&mut state, "cat /app/main.rs").expect("should succeed");
        assert!(cont);
        assert_eq!(out, "fn main() {}");
    }

    // --- cd dispatch ---

    #[test]
    fn dispatch_cd_updates_cwd() {
        let mut state = state_with_files();
        let (cont, _) = dispatch_str(&mut state, "cd /app").expect("should succeed");
        assert!(cont);
        assert_eq!(state.cwd, PathBuf::from("/app"));
    }

    // --- find dispatch ---

    #[test]
    fn dispatch_find_lists_descendants() {
        let mut state = state_with_files();
        let (cont, out) = dispatch_str(&mut state, "find /app").expect("should succeed");
        assert!(cont);
        assert!(out.contains("/app/main.rs"), "got: {out}");
        assert!(out.contains("/app/lib.rs"), "got: {out}");
    }

    // --- help dispatch ---

    #[test]
    fn dispatch_help_contains_commands() {
        let mut state = PreviewState::default();
        let (cont, out) = dispatch_str(&mut state, "help").expect("should succeed");
        assert!(cont);
        assert!(out.contains("ls"), "got: {out}");
        assert!(out.contains("cd"), "got: {out}");
    }

    // --- exit dispatch ---

    #[test]
    fn dispatch_exit_returns_false() {
        let mut state = PreviewState::default();
        let cmd = ParsedCommand::Exit;
        let mut buf = Vec::new();
        let cont = dispatch(&mut state, cmd, &mut buf).expect("should succeed");
        assert!(!cont, "exit must return false to signal loop termination");
    }

    // --- empty dispatch ---

    #[test]
    fn dispatch_empty_returns_true_and_no_output() {
        let mut state = PreviewState::default();
        let (cont, out) = dispatch_str(&mut state, "").expect("should succeed");
        assert!(cont);
        assert_eq!(out, "");
    }

    // --- unknown command ---

    #[test]
    fn dispatch_unknown_returns_unknown_command_error() {
        let mut state = PreviewState::default();
        let result = dispatch_str(&mut state, "rm -rf /");
        assert!(matches!(result, Err(ReplError::UnknownCommand { .. })));
    }

    // --- error propagation ---

    #[test]
    fn dispatch_cat_missing_file_propagates_error() {
        let mut state = PreviewState::default();
        let result = dispatch_str(&mut state, "cat /nonexistent.txt");
        assert!(matches!(result, Err(ReplError::PathNotFound { .. })));
    }

    // --- cat with no argument returns InvalidArguments (not a confusing dir error) ---

    #[test]
    fn dispatch_cat_no_argument_returns_invalid_arguments() {
        let mut state = PreviewState::default();
        let result = dispatch_str(&mut state, "cat");
        assert!(
            matches!(result, Err(ReplError::InvalidArguments { ref command, .. }) if command == "cat"),
            "cat with no args must return InvalidArguments, got: {result:?}"
        );
    }

    // --- ls -l long format propagates through dispatch ---

    #[test]
    fn dispatch_ls_long_format_shows_type_prefix() {
        let mut state = state_with_files();
        state.cwd = PathBuf::from("/app");
        let (cont, out) = dispatch_str(&mut state, "ls -l").expect("ls -l should succeed");
        assert!(cont);
        // Directories get `d` prefix, files get `-` prefix in long format.
        assert!(
            out.contains('d') || out.contains('-'),
            "long format must include type prefix, got: {out}"
        );
    }

    // --- :layers dispatch ---

    #[test]
    fn dispatch_layers_empty_state_prints_no_layers() {
        let mut state = PreviewState::default();
        let (cont, out) = dispatch_str(&mut state, ":layers").expect(":layers should succeed");
        assert!(cont, ":layers must return true (keep REPL running)");
        assert!(out.trim() == "No layers recorded.", "got: {out}");
    }

    #[test]
    fn dispatch_layers_with_data_shows_layer_info() {
        use crate::model::state::LayerSummary;

        let mut state = PreviewState::default();
        state.layers.push(LayerSummary {
            instruction_type: "COPY".to_string(),
            files_changed: vec![PathBuf::from("/app/main.rs")],
            env_changed: vec![],
        });
        let (cont, out) = dispatch_str(&mut state, ":layers").expect(":layers should succeed");
        assert!(cont);
        assert!(out.contains("Layer 1"), "got: {out}");
        assert!(out.contains("COPY"), "got: {out}");
    }

    // --- :installed dispatch ---

    #[test]
    fn dispatch_installed_empty_state_prints_no_packages() {
        let mut state = PreviewState::default();
        let (cont, out) =
            dispatch_str(&mut state, ":installed").expect(":installed should succeed");
        assert!(cont, ":installed must return true (keep REPL running)");
        assert_eq!(out.trim(), "No packages recorded.", "got: {out}");
    }

    #[test]
    fn dispatch_installed_with_packages_shows_manager_line() {
        let mut state = PreviewState::default();
        state.installed.record("apt", "curl".to_string());
        let (cont, out) =
            dispatch_str(&mut state, ":installed").expect(":installed should succeed");
        assert!(cont);
        assert!(out.contains("apt: curl"), "got: {out}");
    }

    // --- which dispatch ---

    #[test]
    fn dispatch_which_found_in_apt_returns_path() {
        let mut state = PreviewState::default();
        state.installed.record("apt", "git".to_string());
        let (cont, out) = dispatch_str(&mut state, "which git").expect("which should succeed");
        assert!(cont);
        assert_eq!(out.trim(), "/usr/bin/git", "got: {out}");
    }

    #[test]
    fn dispatch_which_not_found_returns_error() {
        let mut state = PreviewState::default();
        let result = dispatch_str(&mut state, "which nonexistent");
        assert!(
            matches!(result, Err(ReplError::PathNotFound { .. })),
            "which with unknown cmd must return PathNotFound; got: {result:?}"
        );
    }

    #[test]
    fn dispatch_which_no_arg_returns_invalid_arguments() {
        let mut state = PreviewState::default();
        let result = dispatch_str(&mut state, "which");
        assert!(
            matches!(result, Err(ReplError::InvalidArguments { ref command, .. }) if command == "which"),
            "which with no args must return InvalidArguments; got: {result:?}"
        );
    }

    #[test]
    fn dispatch_which_found_in_pip_returns_local_bin_path() {
        let mut state = PreviewState::default();
        state.installed.record("pip", "flask".to_string());
        let (cont, out) = dispatch_str(&mut state, "which flask").expect("which should succeed");
        assert!(cont);
        assert_eq!(out.trim(), "/usr/local/bin/flask", "got: {out}");
    }

    #[test]
    fn dispatch_installed_multiple_managers_shows_both_lines() {
        let mut state = PreviewState::default();
        state.installed.record("apt", "curl".to_string());
        state.installed.record("pip", "requests".to_string());
        let (cont, out) =
            dispatch_str(&mut state, ":installed").expect(":installed should succeed");
        assert!(cont);
        assert!(out.contains("apt: curl"), "apt line missing; got:\n{out}");
        assert!(
            out.contains("pip: requests"),
            "pip line missing; got:\n{out}"
        );
        // apt must appear before pip in the output.
        let apt_pos = out.find("apt:").expect("apt line must exist");
        let pip_pos = out.find("pip:").expect("pip line must exist");
        assert!(apt_pos < pip_pos, "apt must appear before pip; got:\n{out}");
    }

    // --- apt list dispatch ---

    #[test]
    fn dispatch_apt_list_installed_empty_prints_listing() {
        let mut state = PreviewState::default();
        let (cont, out) = dispatch_str(&mut state, "apt list --installed").expect("should succeed");
        assert!(
            cont,
            "apt list --installed must return true (keep REPL running)"
        );
        assert!(
            out.contains("Listing..."),
            "must contain 'Listing...'; got: {out}"
        );
        assert!(
            out.contains("(no packages recorded)"),
            "empty registry sentinel must appear; got: {out}"
        );
    }

    #[test]
    fn dispatch_apt_list_installed_with_packages() {
        let mut state = PreviewState::default();
        state.installed.record("apt", "curl".to_string());
        let (cont, out) = dispatch_str(&mut state, "apt list --installed").expect("should succeed");
        assert!(cont);
        assert!(
            out.contains("curl/simulated [installed,simulated]"),
            "got: {out}"
        );
    }

    #[test]
    fn dispatch_apt_list_without_installed_flag_prints_usage() {
        let mut state = PreviewState::default();
        let (cont, out) = dispatch_str(&mut state, "apt list").expect("should succeed");
        assert!(cont);
        assert!(
            out.contains("Usage: apt list --installed"),
            "must print usage when --installed flag omitted; got: {out}"
        );
    }

    #[test]
    fn dispatch_apt_bare_is_unknown() {
        let mut state = PreviewState::default();
        let result = dispatch_str(&mut state, "apt");
        assert!(
            matches!(result, Err(ReplError::UnknownCommand { .. })),
            "bare 'apt' must return UnknownCommand; got: {result:?}"
        );
    }

    // --- pip list dispatch ---

    #[test]
    fn dispatch_pip_list_empty_prints_header() {
        let mut state = PreviewState::default();
        let (cont, out) = dispatch_str(&mut state, "pip list").expect("should succeed");
        assert!(cont, "pip list must return true (keep REPL running)");
        assert!(
            out.contains("Package    Version"),
            "header missing; got: {out}"
        );
        assert!(
            out.contains("---------- -------"),
            "separator missing; got: {out}"
        );
        // No data rows should appear when registry is empty.
        assert!(
            !out.contains("(simulated)"),
            "must not print data rows for empty registry; got: {out}"
        );
    }

    #[test]
    fn dispatch_pip_list_with_packages() {
        let mut state = PreviewState::default();
        state.installed.record("pip", "flask".to_string());
        let (cont, out) = dispatch_str(&mut state, "pip list").expect("should succeed");
        assert!(cont);
        // Assert the combined, column-aligned row so a split-line regression
        // would not pass. "flask" (5 chars) padded to 10 + 1 space = 6 spaces.
        assert!(
            out.contains("flask      (simulated)"),
            "must show column-aligned row for flask; got: {out}"
        );
    }

    #[test]
    fn dispatch_pip_bare_is_unknown() {
        let mut state = PreviewState::default();
        let result = dispatch_str(&mut state, "pip");
        assert!(
            matches!(result, Err(ReplError::UnknownCommand { .. })),
            "bare 'pip' must return UnknownCommand; got: {result:?}"
        );
    }

    // --- :reload parse layer ---
    //
    // `:reload` is intercepted in `run_repl` before `dispatch` is called, so
    // `dispatch` will never see it in normal operation (it panics via
    // `unreachable!` if it does). The parse-layer test below is sufficient to
    // confirm that the parser recognises the command correctly.

    #[test]
    fn reload_is_recognized_by_parse_input() {
        assert_eq!(parse_input(":reload"), ParsedCommand::Reload);
    }

    // --- :explain dispatch ---

    #[test]
    fn dispatch_explain_existing_file_shows_provenance() {
        use crate::model::provenance::ProvenanceSource;

        let mut state = PreviewState::default();
        // Insert a file with CopyFromHost provenance.
        let node = FsNode::File(crate::model::fs::FileNode {
            content: vec![],
            provenance: crate::model::provenance::Provenance::new(ProvenanceSource::CopyFromHost {
                host_path: PathBuf::from("src/main.rs"),
            }),
            permissions: None,
        });
        state.fs.insert(PathBuf::from("/app/main.rs"), node);

        let (cont, out) =
            dispatch_str(&mut state, ":explain /app/main.rs").expect(":explain should succeed");
        assert!(cont, ":explain must return true (keep REPL running)");
        assert!(
            out.contains("COPY from host: src/main.rs"),
            "output must show provenance; got: {out}"
        );
        assert!(
            out.contains("Created by:"),
            "output must contain 'Created by:' label; got: {out}"
        );
    }

    #[test]
    fn dispatch_explain_nonexistent_path_returns_path_not_found() {
        let mut state = PreviewState::default();
        let result = dispatch_str(&mut state, ":explain /no/such/file.txt");
        assert!(
            matches!(result, Err(ReplError::PathNotFound { .. })),
            ":explain on missing path must return PathNotFound; got: {result:?}"
        );
    }

    #[test]
    fn dispatch_explain_no_argument_returns_invalid_arguments() {
        let mut state = PreviewState::default();
        let result = dispatch_str(&mut state, ":explain");
        assert!(
            matches!(result, Err(ReplError::InvalidArguments { ref command, .. }) if command == ":explain"),
            ":explain with no args must return InvalidArguments; got: {result:?}"
        );
    }

    // --- :history dispatch ---

    #[test]
    fn dispatch_history_empty_state_prints_no_history() {
        let mut state = PreviewState::default();
        let (cont, out) = dispatch_str(&mut state, ":history").expect(":history should succeed");
        assert!(cont, ":history must return true (keep REPL running)");
        assert!(out.trim() == "No history recorded.", "got: {out}");
    }

    #[test]
    fn dispatch_history_with_data_shows_entry_info() {
        use crate::model::state::HistoryEntry;

        let mut state = PreviewState::default();
        state.history.push(HistoryEntry {
            line: 5,
            instruction: "RUN echo hello".to_string(),
            effect: "simulated shell command".to_string(),
        });
        let (cont, out) = dispatch_str(&mut state, ":history").expect(":history should succeed");
        assert!(cont);
        assert!(out.contains("RUN echo hello"), "got: {out}");
        assert!(out.contains("simulated shell command"), "got: {out}");
    }
}
