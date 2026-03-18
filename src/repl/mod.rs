// Interactive REPL shell loop for demu.
//
// `run_repl` is the single public entry point. It owns the rustyline editor,
// reads lines from the terminal, dispatches each parsed command to the
// appropriate handler, and manages the session lifecycle (Ctrl-C, Ctrl-D,
// and the `exit` command).

pub mod commands;
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
use crate::repl::commands::{cat, cd, env_cmd, find, help, ls, pwd};
use crate::repl::custom::{history, layers};
use crate::repl::error::ReplError;
use crate::repl::parse::{parse_input, ParsedCommand};

/// Run the interactive REPL until the user exits.
///
/// The REPL:
/// - Displays a dynamic prompt showing the current working directory.
/// - Parses each input line into a [`ParsedCommand`].
/// - Dispatches to the appropriate command handler.
/// - Prints errors from handlers as compact terminal messages (non-fatal).
/// - Exits gracefully on Ctrl-D, `exit`, or `quit`.
/// - Continues on Ctrl-C (prints a hint on how to exit).
pub fn run_repl(state: &mut PreviewState) -> anyhow::Result<()> {
    let mut editor = DefaultEditor::new()?;
    let stdout = io::stdout();

    loop {
        // Build the prompt from the current working directory.
        let prompt = format!("demu:{}$ ", state.cwd.display());

        match editor.readline(&prompt) {
            Ok(line) => {
                // Add non-empty lines to history for up-arrow recall.
                if !line.trim().is_empty() {
                    let _ = editor.add_history_entry(&line);
                }

                let cmd = parse_input(&line);
                let mut out = stdout.lock();

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

/// Strip terminal-unsafe characters from a string before printing to stderr.
///
/// Removes:
/// - C0 control characters: U+0000–U+001F (includes ESC, NUL, CR, LF, TAB)
/// - DEL: U+007F
/// - C1 control characters: U+0080–U+009F (includes CSI U+009B, which some
///   terminal emulators treat as an ANSI escape sequence introducer)
///
/// This prevents terminal escape injection when echoing user-supplied input.
fn sanitize_for_terminal(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            let cp = c as u32;
            // Allow printable ASCII and all codepoints above the C1 range.
            !(cp <= 0x1F || cp == 0x7F || (0x80..=0x9F).contains(&cp))
        })
        .collect()
}

/// Dispatch a parsed command to the appropriate handler.
///
/// All handlers write their output to `writer` rather than to stdout, which
/// makes this function fully testable without capturing stdout.
///
/// Returns `Ok(true)` normally or `Ok(false)` when the command signals exit.
/// The caller is responsible for exiting the loop on `Exit`.
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
        ParsedCommand::Exit => {
            return Ok(false);
        }
        ParsedCommand::Empty => {
            // Nothing to do — just re-prompt.
        }
        ParsedCommand::Unknown { input } => {
            return Err(ReplError::UnknownCommand { input });
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
