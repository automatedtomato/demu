// Input parsing for the REPL command layer.
//
// `parse_input` converts a raw line of text into a typed `ParsedCommand`,
// isolating all string-splitting and flag detection in one place so that
// command handlers receive structured arguments rather than raw strings.

/// A fully-parsed REPL command ready for dispatch.
///
/// Each variant carries exactly the information the corresponding command
/// handler needs. Parsing is pure and infallible: unrecognised input becomes
/// `Unknown` rather than an error so the REPL loop can print a friendly
/// message.
#[derive(Debug, PartialEq)]
pub enum ParsedCommand {
    /// `ls [-l|-la|-al] [path]` — list directory contents.
    Ls {
        /// Optional path to list; `None` means current working directory.
        path: Option<String>,
        /// Whether long format (`-l` / `-la` / `-al`) was requested.
        long: bool,
    },
    /// `cd [path]` — change working directory. Defaults to `/` when omitted.
    Cd { path: String },
    /// `pwd` — print working directory.
    Pwd,
    /// `cat <path>` — print file contents.
    Cat { path: String },
    /// `find <path> [-name <pattern>]` — search the filesystem.
    Find {
        /// Root path to search from.
        path: String,
        /// Optional glob pattern for the filename (e.g. `*.rs`).
        name_pattern: Option<String>,
    },
    /// `env` — print all environment variables.
    Env,
    /// `exit` — leave the REPL.
    Exit,
    /// `help` — display command reference.
    Help,
    /// `:layers` — display a layer-by-layer summary of changes.
    Layers,
    /// `:history` — display the instruction history timeline.
    History,
    /// `:installed` — list all recorded packages grouped by manager.
    Installed,
    /// `which <cmd>` — show the simulated binary path for a command.
    Which {
        /// The command name to look up. Empty string when no argument was given.
        cmd: String,
    },
    /// `apt list [--installed]` — list simulated apt packages.
    AptList {
        /// Whether `--installed` flag was given.
        installed: bool,
    },
    /// `pip list` — list simulated pip packages.
    PipList,
    /// The input was empty or only whitespace.
    Empty,
    /// The input did not match any known command.
    Unknown { input: String },
}

/// Parse a raw line of user input into a typed [`ParsedCommand`].
///
/// Parsing rules:
/// - Empty / whitespace-only → [`ParsedCommand::Empty`]
/// - Leading word is matched case-sensitively against known commands.
/// - Unknown leading word → [`ParsedCommand::Unknown`]
/// - Flag detection for `ls` (`-l`, `-la`, `-al`, `-a`) sets `long = true`.
/// - `cd` with no argument defaults to `"/"`.
/// - `find` accepts optional `-name <pattern>` pair after the path.
pub fn parse_input(line: &str) -> ParsedCommand {
    let trimmed = line.trim();

    // Empty input — nothing to dispatch.
    if trimmed.is_empty() {
        return ParsedCommand::Empty;
    }

    // Split into whitespace-separated tokens.
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let command = tokens[0];
    let args = &tokens[1..];

    match command {
        "ls" => parse_ls(args),
        "cd" => parse_cd(args),
        "pwd" => ParsedCommand::Pwd,
        "cat" => parse_cat(args),
        "find" => parse_find(args),
        "env" => ParsedCommand::Env,
        "exit" | "quit" => ParsedCommand::Exit,
        "help" => ParsedCommand::Help,
        // Standard commands with arguments.
        "apt" => parse_apt(args, trimmed),
        "pip" => parse_pip(args, trimmed),
        "which" => parse_which(args),
        // Colon-prefixed custom inspection commands.
        ":layers" => ParsedCommand::Layers,
        ":history" => ParsedCommand::History,
        ":installed" => ParsedCommand::Installed,
        _ => ParsedCommand::Unknown {
            input: trimmed.to_string(),
        },
    }
}

/// Parse arguments for the `ls` command.
///
/// Recognises `-l`, `-la`, `-al`, `-a` as long-format flags. All other tokens
/// that don't start with `-` are treated as the path argument. The last
/// non-flag token wins if multiple appear (unusual but handled gracefully).
fn parse_ls(args: &[&str]) -> ParsedCommand {
    let mut long = false;
    let mut path: Option<String> = None;

    for &token in args {
        if token.starts_with('-') {
            // Any flag containing 'l' enables long format.
            // Covers -l, -la, -al, -lh, etc.
            if token.contains('l') {
                long = true;
            }
        } else {
            path = Some(token.to_string());
        }
    }

    ParsedCommand::Ls { path, long }
}

/// Parse arguments for the `cd` command.
///
/// Defaults to `"/"` when no argument is provided, mirroring the behaviour of
/// shells that default to `$HOME` (we use `/` since there is no home concept).
fn parse_cd(args: &[&str]) -> ParsedCommand {
    let path = args
        .first()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "/".to_string());
    ParsedCommand::Cd { path }
}

/// Parse arguments for the `cat` command.
///
/// Joins all remaining tokens with a space to support paths with spaces,
/// though spaces in paths are uncommon in container filesystems.
fn parse_cat(args: &[&str]) -> ParsedCommand {
    let path = if args.is_empty() {
        String::new()
    } else {
        args.join(" ")
    };
    ParsedCommand::Cat { path }
}

/// Parse arguments for the `find` command.
///
/// Expected forms:
/// - `find <path>`
/// - `find <path> -name <pattern>`
///
/// The path defaults to `"/"` when omitted. Any `-name` flag must be
/// immediately followed by its pattern; extra tokens are ignored.
fn parse_find(args: &[&str]) -> ParsedCommand {
    let mut path = "/".to_string();
    let mut name_pattern: Option<String> = None;

    let mut i = 0;
    // First non-flag argument is the search root.
    if i < args.len() && !args[i].starts_with('-') {
        path = args[i].to_string();
        i += 1;
    }

    // Scan remaining tokens for -name <pattern>.
    while i < args.len() {
        if args[i] == "-name" {
            i += 1;
            if i < args.len() {
                name_pattern = Some(args[i].to_string());
            }
        }
        i += 1;
    }

    ParsedCommand::Find { path, name_pattern }
}

/// Parse arguments for the `apt` command.
///
/// Only `apt list` and `apt list --installed` are modeled. All other `apt`
/// sub-commands (install, update, upgrade, etc.) produce `Unknown` so the
/// REPL can surface them as unsupported rather than silently ignoring them.
///
/// `trimmed` is the full trimmed input line, used for the `Unknown` variant.
fn parse_apt(args: &[&str], trimmed: &str) -> ParsedCommand {
    match args {
        ["list", "--installed"] => ParsedCommand::AptList { installed: true },
        ["list"] => ParsedCommand::AptList { installed: false },
        // All other forms (bare apt, apt install, apt update, …) are unknown.
        _ => ParsedCommand::Unknown {
            input: trimmed.to_string(),
        },
    }
}

/// Parse arguments for the `pip` command.
///
/// Only `pip list` is modeled. All other `pip` sub-commands (install, freeze,
/// show, etc.) produce `Unknown` so the REPL can surface them appropriately.
///
/// `trimmed` is the full trimmed input line, used for the `Unknown` variant.
fn parse_pip(args: &[&str], trimmed: &str) -> ParsedCommand {
    match args {
        ["list"] => ParsedCommand::PipList,
        // All other forms (bare pip, pip install, …) are unknown.
        _ => ParsedCommand::Unknown {
            input: trimmed.to_string(),
        },
    }
}

/// Parse arguments for the `which` command.
///
/// Expects exactly one positional argument: the command name to look up.
/// When no argument is present, `cmd` is set to `String::new()` so the
/// handler can return a specific `InvalidArguments` error rather than a
/// generic parse error.
fn parse_which(args: &[&str]) -> ParsedCommand {
    let cmd = args.first().map(|s| s.to_string()).unwrap_or_default();
    ParsedCommand::Which { cmd }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    // --- Empty input ---

    #[test]
    fn empty_string_returns_empty() {
        assert_eq!(parse_input(""), ParsedCommand::Empty);
    }

    #[test]
    fn whitespace_only_returns_empty() {
        assert_eq!(parse_input("   "), ParsedCommand::Empty);
    }

    #[test]
    fn tab_only_returns_empty() {
        assert_eq!(parse_input("\t"), ParsedCommand::Empty);
    }

    // --- ls ---

    #[test]
    fn ls_bare_returns_ls_with_no_path_no_long() {
        assert_eq!(
            parse_input("ls"),
            ParsedCommand::Ls {
                path: None,
                long: false
            }
        );
    }

    #[test]
    fn ls_with_path_no_flags() {
        assert_eq!(
            parse_input("ls /app"),
            ParsedCommand::Ls {
                path: Some("/app".to_string()),
                long: false
            }
        );
    }

    #[test]
    fn ls_l_flag_sets_long() {
        assert_eq!(
            parse_input("ls -l"),
            ParsedCommand::Ls {
                path: None,
                long: true
            }
        );
    }

    #[test]
    fn ls_la_flag_sets_long() {
        assert_eq!(
            parse_input("ls -la /app"),
            ParsedCommand::Ls {
                path: Some("/app".to_string()),
                long: true
            }
        );
    }

    #[test]
    fn ls_al_flag_sets_long() {
        assert_eq!(
            parse_input("ls -al /app"),
            ParsedCommand::Ls {
                path: Some("/app".to_string()),
                long: true
            }
        );
    }

    #[test]
    fn ls_path_before_flag() {
        // ls /app -l — unusual ordering but should still work
        assert_eq!(
            parse_input("ls /app -l"),
            ParsedCommand::Ls {
                path: Some("/app".to_string()),
                long: true
            }
        );
    }

    // --- cd ---

    #[test]
    fn cd_with_path() {
        assert_eq!(
            parse_input("cd /app"),
            ParsedCommand::Cd {
                path: "/app".to_string()
            }
        );
    }

    #[test]
    fn cd_no_args_defaults_to_root() {
        assert_eq!(
            parse_input("cd"),
            ParsedCommand::Cd {
                path: "/".to_string()
            }
        );
    }

    #[test]
    fn cd_relative_path() {
        assert_eq!(
            parse_input("cd src"),
            ParsedCommand::Cd {
                path: "src".to_string()
            }
        );
    }

    #[test]
    fn cd_dotdot() {
        assert_eq!(
            parse_input("cd .."),
            ParsedCommand::Cd {
                path: "..".to_string()
            }
        );
    }

    // --- pwd ---

    #[test]
    fn pwd_returns_pwd() {
        assert_eq!(parse_input("pwd"), ParsedCommand::Pwd);
    }

    #[test]
    fn pwd_with_trailing_whitespace() {
        assert_eq!(parse_input("pwd  "), ParsedCommand::Pwd);
    }

    // --- cat ---

    #[test]
    fn cat_with_path() {
        assert_eq!(
            parse_input("cat /app/main.rs"),
            ParsedCommand::Cat {
                path: "/app/main.rs".to_string()
            }
        );
    }

    #[test]
    fn cat_with_relative_path() {
        assert_eq!(
            parse_input("cat README.md"),
            ParsedCommand::Cat {
                path: "README.md".to_string()
            }
        );
    }

    #[test]
    fn cat_no_args_returns_empty_path() {
        // cat with no args — handler will produce an error, parser just records empty path.
        assert_eq!(
            parse_input("cat"),
            ParsedCommand::Cat {
                path: String::new()
            }
        );
    }

    // --- find ---

    #[test]
    fn find_with_path_only() {
        assert_eq!(
            parse_input("find /"),
            ParsedCommand::Find {
                path: "/".to_string(),
                name_pattern: None
            }
        );
    }

    #[test]
    fn find_with_name_flag() {
        assert_eq!(
            parse_input("find / -name *.rs"),
            ParsedCommand::Find {
                path: "/".to_string(),
                name_pattern: Some("*.rs".to_string())
            }
        );
    }

    #[test]
    fn find_no_args_defaults_to_root() {
        assert_eq!(
            parse_input("find"),
            ParsedCommand::Find {
                path: "/".to_string(),
                name_pattern: None
            }
        );
    }

    #[test]
    fn find_non_root_path_with_name() {
        assert_eq!(
            parse_input("find /app -name *.txt"),
            ParsedCommand::Find {
                path: "/app".to_string(),
                name_pattern: Some("*.txt".to_string())
            }
        );
    }

    // --- env ---

    #[test]
    fn env_returns_env() {
        assert_eq!(parse_input("env"), ParsedCommand::Env);
    }

    // --- exit / quit ---

    #[test]
    fn exit_returns_exit() {
        assert_eq!(parse_input("exit"), ParsedCommand::Exit);
    }

    #[test]
    fn quit_returns_exit() {
        assert_eq!(parse_input("quit"), ParsedCommand::Exit);
    }

    // --- help ---

    #[test]
    fn help_returns_help() {
        assert_eq!(parse_input("help"), ParsedCommand::Help);
    }

    // --- Unknown ---

    #[test]
    fn unknown_command_returns_unknown_with_full_input() {
        assert_eq!(
            parse_input("frobnicate foo bar"),
            ParsedCommand::Unknown {
                input: "frobnicate foo bar".to_string()
            }
        );
    }

    #[test]
    fn unknown_single_word_returns_unknown() {
        assert_eq!(
            parse_input("rm"),
            ParsedCommand::Unknown {
                input: "rm".to_string()
            }
        );
    }

    // --- Case sensitivity ---

    #[test]
    fn ls_uppercase_is_unknown() {
        // Commands are case-sensitive — LS is not ls.
        assert_eq!(
            parse_input("LS"),
            ParsedCommand::Unknown {
                input: "LS".to_string()
            }
        );
    }

    // --- :layers ---

    #[test]
    fn layers_bare_returns_layers() {
        assert_eq!(parse_input(":layers"), ParsedCommand::Layers);
    }

    #[test]
    fn layers_with_trailing_whitespace_returns_layers() {
        assert_eq!(parse_input(":layers  "), ParsedCommand::Layers);
    }

    #[test]
    fn layers_uppercase_is_unknown() {
        // Custom commands are case-sensitive — :LAYERS is not :layers.
        assert_eq!(
            parse_input(":LAYERS"),
            ParsedCommand::Unknown {
                input: ":LAYERS".to_string()
            }
        );
    }

    // --- :history ---

    #[test]
    fn history_bare_returns_history() {
        assert_eq!(parse_input(":history"), ParsedCommand::History);
    }

    #[test]
    fn history_with_trailing_whitespace_returns_history() {
        assert_eq!(parse_input(":history  "), ParsedCommand::History);
    }

    #[test]
    fn history_uppercase_is_unknown() {
        assert_eq!(
            parse_input(":HISTORY"),
            ParsedCommand::Unknown {
                input: ":HISTORY".to_string()
            }
        );
    }

    // --- Unimplemented colon commands are unknown ---

    #[test]
    fn explain_is_unknown() {
        // :explain is not yet dispatched — must be unknown.
        assert_eq!(
            parse_input(":explain /app/main.rs"),
            ParsedCommand::Unknown {
                input: ":explain /app/main.rs".to_string()
            }
        );
    }

    // --- :installed ---

    #[test]
    fn installed_bare_returns_installed() {
        assert_eq!(parse_input(":installed"), ParsedCommand::Installed);
    }

    #[test]
    fn installed_with_trailing_whitespace() {
        assert_eq!(parse_input(":installed  "), ParsedCommand::Installed);
    }

    #[test]
    fn installed_uppercase_is_unknown() {
        // Custom commands are case-sensitive.
        assert_eq!(
            parse_input(":INSTALLED"),
            ParsedCommand::Unknown {
                input: ":INSTALLED".to_string()
            }
        );
    }

    // --- apt ---

    #[test]
    fn apt_list_installed_returns_apt_list_installed_true() {
        assert_eq!(
            parse_input("apt list --installed"),
            ParsedCommand::AptList { installed: true }
        );
    }

    #[test]
    fn apt_list_returns_apt_list_installed_false() {
        assert_eq!(
            parse_input("apt list"),
            ParsedCommand::AptList { installed: false }
        );
    }

    #[test]
    fn apt_bare_returns_unknown() {
        assert_eq!(
            parse_input("apt"),
            ParsedCommand::Unknown {
                input: "apt".to_string()
            }
        );
    }

    #[test]
    fn apt_install_returns_unknown() {
        assert_eq!(
            parse_input("apt install curl"),
            ParsedCommand::Unknown {
                input: "apt install curl".to_string()
            }
        );
    }

    #[test]
    fn apt_update_returns_unknown() {
        assert_eq!(
            parse_input("apt update"),
            ParsedCommand::Unknown {
                input: "apt update".to_string()
            }
        );
    }

    #[test]
    fn apt_uppercase_is_unknown() {
        // Commands are case-sensitive — APT is not apt.
        assert_eq!(
            parse_input("APT list --installed"),
            ParsedCommand::Unknown {
                input: "APT list --installed".to_string()
            }
        );
    }

    #[test]
    fn apt_list_installed_with_extra_args_is_unknown() {
        // Extra tokens after --installed are not recognised; the parser rejects
        // to Unknown so the REPL surfaces an unsupported-command message.
        assert_eq!(
            parse_input("apt list --installed --verbose"),
            ParsedCommand::Unknown {
                input: "apt list --installed --verbose".to_string()
            }
        );
    }

    // --- pip ---

    #[test]
    fn pip_list_returns_pip_list() {
        assert_eq!(parse_input("pip list"), ParsedCommand::PipList);
    }

    #[test]
    fn pip_bare_returns_unknown() {
        assert_eq!(
            parse_input("pip"),
            ParsedCommand::Unknown {
                input: "pip".to_string()
            }
        );
    }

    #[test]
    fn pip_install_returns_unknown() {
        assert_eq!(
            parse_input("pip install requests"),
            ParsedCommand::Unknown {
                input: "pip install requests".to_string()
            }
        );
    }

    #[test]
    fn pip_uppercase_is_unknown() {
        // Commands are case-sensitive — PIP is not pip.
        assert_eq!(
            parse_input("PIP list"),
            ParsedCommand::Unknown {
                input: "PIP list".to_string()
            }
        );
    }

    #[test]
    fn pip_list_with_extra_flag_is_unknown() {
        // `pip list --outdated` is not a modeled command; extra flags after
        // `list` fall through to Unknown so the REPL surfaces the rejection.
        assert_eq!(
            parse_input("pip list --outdated"),
            ParsedCommand::Unknown {
                input: "pip list --outdated".to_string()
            }
        );
    }

    // --- which ---

    #[test]
    fn which_with_cmd_returns_which() {
        assert_eq!(
            parse_input("which curl"),
            ParsedCommand::Which {
                cmd: "curl".to_string()
            }
        );
    }

    #[test]
    fn which_no_args_returns_which_empty_cmd() {
        assert_eq!(
            parse_input("which"),
            ParsedCommand::Which { cmd: String::new() }
        );
    }

    #[test]
    fn which_uppercase_is_unknown() {
        // Commands are case-sensitive — WHICH is not which.
        assert_eq!(
            parse_input("WHICH curl"),
            ParsedCommand::Unknown {
                input: "WHICH curl".to_string()
            }
        );
    }
}
