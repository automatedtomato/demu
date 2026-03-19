// `help` command — display a formatted command reference table.
//
// Prints a compact, terminal-friendly table of all REPL commands. This is
// a pure write operation that never fails in meaningful ways.

use std::io::Write;

use crate::model::state::PreviewState;
use crate::repl::error::ReplError;

/// Execute the `help` command.
///
/// Writes a formatted table of all available commands to `writer`. The table
/// includes each command name, its argument signature, and a brief description.
pub fn execute(_state: &PreviewState, writer: &mut impl Write) -> Result<(), ReplError> {
    // The help text uses fixed-width columns for terminal alignment.
    // Commands are listed in logical usage order.
    let help_text = "\
demu — Docker preview shell

COMMANDS
  ls [-l] [path]           list directory contents
  cd [path]                change working directory (default: /)
  pwd                      print working directory
  cat <path>               print file contents
  find <path> [-name pat]  search filesystem for files
  env                      print environment variables
  apt list --installed     list simulated apt packages
  pip list                 list simulated pip packages
  which <cmd>              show simulated binary path for a command
  help                     show this help message
  exit                     leave the REPL

CUSTOM COMMANDS (prefix with :)
  :explain <path>          show where a file came from
  :layers                  show layer-by-layer changes
  :history                 show instruction history
  :installed               show simulated package installs
  :warnings                show simulation warnings
  :reload                  re-read and re-process the Dockerfile

NOTE: demu is a preview shell. Commands show simulated state, not real containers.
";
    write!(writer, "{help_text}").map_err(|e| ReplError::InvalidArguments {
        command: "help".to_string(),
        message: e.to_string(),
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::state::PreviewState;

    fn run() -> String {
        let state = PreviewState::default();
        let mut buf = Vec::new();
        execute(&state, &mut buf).expect("help should not fail");
        String::from_utf8(buf).expect("utf-8")
    }

    // --- All commands appear ---

    #[test]
    fn help_contains_ls() {
        assert!(run().contains("ls"), "output must mention 'ls'");
    }

    #[test]
    fn help_contains_cd() {
        assert!(run().contains("cd"), "output must mention 'cd'");
    }

    #[test]
    fn help_contains_pwd() {
        assert!(run().contains("pwd"), "output must mention 'pwd'");
    }

    #[test]
    fn help_contains_cat() {
        assert!(run().contains("cat"), "output must mention 'cat'");
    }

    #[test]
    fn help_contains_find() {
        assert!(run().contains("find"), "output must mention 'find'");
    }

    #[test]
    fn help_contains_env() {
        assert!(run().contains("env"), "output must mention 'env'");
    }

    #[test]
    fn help_contains_exit() {
        assert!(run().contains("exit"), "output must mention 'exit'");
    }

    #[test]
    fn help_contains_help() {
        assert!(run().contains("help"), "output must mention 'help'");
    }

    // --- Output is non-trivial ---

    #[test]
    fn help_output_is_non_empty() {
        assert!(!run().is_empty(), "help output must not be empty");
    }

    #[test]
    fn help_returns_ok() {
        let state = PreviewState::default();
        let mut buf = Vec::new();
        assert!(execute(&state, &mut buf).is_ok());
    }

    // --- New commands from issue #23 ---

    #[test]
    fn help_contains_apt_list_installed() {
        assert!(
            run().contains("apt list --installed"),
            "output must mention 'apt list --installed'"
        );
    }

    #[test]
    fn help_contains_pip_list() {
        assert!(run().contains("pip list"), "output must mention 'pip list'");
    }

    #[test]
    fn help_contains_reload() {
        assert!(
            run().contains(":reload"),
            "output must mention ':reload' in the custom commands section"
        );
    }
}
