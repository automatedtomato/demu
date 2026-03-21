// User-visible warnings emitted during Dockerfile simulation.
//
// Warnings are collected in `PreviewState` and surfaced via the REPL so the
// user understands where the simulation is approximate or incomplete.
// They are never fatal — the engine continues after recording a warning.

use std::fmt;
use std::path::PathBuf;

/// Describes *why* a RUN sub-command was not modeled by the engine.
///
/// This enum is embedded in `Warning::UnmodeledRunCommand` so the user can
/// distinguish between commands the engine has never heard of, commands that
/// use a flag the engine does not support, and commands that follow a usage
/// pattern that falls outside the modeled subset.
#[derive(Debug, Clone, PartialEq)]
pub enum UnmodeledReason {
    /// The leading token (the command name) is not in the modeled command set.
    ///
    /// Example: `echo hello`, `curl https://example.com | bash`, `make build`.
    UnrecognisedCommand,

    /// The command is known but a specific flag prevents modeling.
    ///
    /// Used for dry-run / simulate flags on package managers (`--dry-run`,
    /// `--simulate`, `-s`, `--just-print`) where the flag semantics change the
    /// operation such that recording packages would be misleading.
    UnsupportedFlag {
        /// The specific flag token that triggered this variant (e.g. `"--dry-run"`).
        flag: String,
    },

    /// The command is known but the argument pattern is outside the modeled subset.
    ///
    /// Examples: `cp /a /b /c` (three positional args), `mkdir /deep/path`
    /// without `-p` when the parent is absent, `mv` with wrong arg count.
    UnsupportedUsage,
}

impl fmt::Display for UnmodeledReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnmodeledReason::UnrecognisedCommand => write!(f, "not modeled"),
            UnmodeledReason::UnsupportedFlag { flag } => {
                write!(f, "unsupported flag: {}", flag)
            }
            UnmodeledReason::UnsupportedUsage => write!(f, "unsupported usage"),
        }
    }
}

/// A non-fatal diagnostic produced by the simulation engine.
///
/// Each variant corresponds to a condition that the engine cannot fully model
/// but wants to surface to the user rather than silently ignore.
#[derive(Debug, Clone, PartialEq)]
pub enum Warning {
    /// A Dockerfile instruction was encountered that the engine does not support.
    UnsupportedInstruction {
        /// The instruction keyword (e.g. "HEALTHCHECK", "ENTRYPOINT").
        instruction: String,
        /// Source line number (1-based) where the instruction appeared.
        line: usize,
    },

    /// A COPY source path could not be located in the build context.
    MissingCopySource {
        /// The path that was referenced but not found.
        path: PathBuf,
    },

    /// A glob pattern in a COPY instruction is not yet modeled by the engine.
    UnsupportedGlob {
        /// The raw glob pattern string.
        pattern: String,
    },

    /// A RUN command contains a sub-command or flag the engine does not simulate.
    UnmodeledRunCommand {
        /// The full raw command text from the Dockerfile.
        command: String,
        /// Explanation for why this command was not modeled.
        reason: UnmodeledReason,
    },

    /// A package install was simulated (recorded in the registry) but not actually executed.
    SimulatedInstall {
        /// The package manager that handled the install (e.g. "apt", "pip").
        manager: String,
        /// The list of package names that were recorded.
        packages: Vec<String>,
    },

    /// A FROM instruction referenced an image name the engine has no stub for.
    EmptyBaseImage {
        /// The image name from the FROM instruction.
        image: String,
    },

    /// A `COPY --from=<stage>` referenced a stage name that is not in the registry.
    ///
    /// This is emitted when the stage lookup in `StageRegistry::get` returns `None`.
    /// The copy is skipped rather than crashing the engine.
    MissingCopyStage {
        /// The stage name or numeric index that was not found.
        stage: String,
        /// Source line number (1-based) where the COPY instruction appeared.
        line: usize,
    },

    /// A Compose service uses `image:` but has no `build:` context.
    ///
    /// The preview filesystem starts empty because the engine does not extract
    /// real image filesystems. The REPL remains functional but shows no files.
    ImageOnlyService {
        /// The image reference from the service's `image:` field.
        image: String,
    },

    /// An `env_file` referenced in a Compose service definition was not found.
    ///
    /// The missing file is skipped and the remaining env_files and environment
    /// entries are still applied. The REPL continues normally.
    EnvFileNotFound {
        /// The path that was referenced but could not be read.
        path: PathBuf,
    },

    /// A Compose `environment` entry used the bare `KEY` form (no value).
    ///
    /// At preview time the host environment is not available, so the key is
    /// skipped rather than inserted with an empty or incorrect value.
    UnresolvedEnvKey {
        /// The environment variable key that had no value.
        key: String,
    },

    /// A Compose `working_dir` with `..` components would have escaped the
    /// virtual filesystem root `/`.
    ///
    /// The path is clamped to `/` and the REPL continues. The user should be
    /// aware that the CWD they see may differ from what Docker would compute.
    WorkdirEscapedRoot {
        /// The raw (unnormalized) path from the Compose YAML.
        path: PathBuf,
    },
}

/// # Terminal output safety
///
/// The output of this `Display` implementation contains user-supplied content
/// (image names, instruction text, file paths, command strings) that has NOT been
/// sanitized for terminal output. Callers **must** apply `sanitize_for_terminal`
/// before writing to a terminal writer. See `src/main.rs` and `src/repl/mod.rs`
/// for the established pattern.
impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::UnsupportedInstruction { instruction, line } => {
                write!(
                    f,
                    "unsupported instruction '{}' at line {} (skipped)",
                    instruction, line
                )
            }
            Warning::MissingCopySource { path } => {
                write!(
                    f,
                    "COPY source '{}' not found in build context (skipped)",
                    path.display()
                )
            }
            Warning::UnsupportedGlob { pattern } => {
                write!(f, "glob pattern '{}' is not modeled; copy skipped", pattern)
            }
            Warning::UnmodeledRunCommand { command, reason } => {
                // Format: "skipped RUN sub-command '{command}' ({reason_detail})"
                // The first word of the command is embedded in the reason detail for
                // UnrecognisedCommand so the user can quickly see which binary was skipped.
                let detail = match reason {
                    UnmodeledReason::UnrecognisedCommand => {
                        let first_word = command.split_whitespace().next().unwrap_or(command);
                        format!("not modeled: {}", first_word)
                    }
                    UnmodeledReason::UnsupportedFlag { flag } => {
                        format!("unsupported flag: {}", flag)
                    }
                    UnmodeledReason::UnsupportedUsage => "unsupported usage".to_string(),
                };
                write!(f, "skipped RUN sub-command '{}' ({})", command, detail)
            }
            Warning::SimulatedInstall { manager, packages } => {
                write!(
                    f,
                    "simulated {} install: {} (no real packages downloaded)",
                    manager,
                    packages.join(", ")
                )
            }
            Warning::EmptyBaseImage { image } => {
                write!(
                    f,
                    "base image '{}' has no stub; filesystem starts empty",
                    image
                )
            }
            Warning::MissingCopyStage { stage, line } => {
                write!(
                    f,
                    "COPY --from='{}' references unknown stage at line {} (skipped)",
                    stage, line
                )
            }
            Warning::ImageOnlyService { image } => {
                write!(
                    f,
                    "service uses image '{}' with no build context — filesystem is empty",
                    image
                )
            }
            Warning::EnvFileNotFound { path } => {
                write!(f, "env_file '{}' not found — skipped", path.display())
            }
            Warning::UnresolvedEnvKey { key } => {
                write!(f, "environment key '{}' has no value — skipped", key)
            }
            Warning::WorkdirEscapedRoot { path } => {
                write!(
                    f,
                    "working_dir '{}' escapes the virtual root — clamped to /",
                    path.display()
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- Variant construction ---

    #[test]
    fn unsupported_instruction_stores_fields() {
        let w = Warning::UnsupportedInstruction {
            instruction: "HEALTHCHECK".to_string(),
            line: 12,
        };
        assert_eq!(
            w,
            Warning::UnsupportedInstruction {
                instruction: "HEALTHCHECK".to_string(),
                line: 12
            }
        );
    }

    #[test]
    fn missing_copy_source_stores_path() {
        let path = PathBuf::from("/build/missing.txt");
        let w = Warning::MissingCopySource { path: path.clone() };
        assert_eq!(w, Warning::MissingCopySource { path });
    }

    #[test]
    fn unsupported_glob_stores_pattern() {
        let w = Warning::UnsupportedGlob {
            pattern: "src/**/*.py".to_string(),
        };
        assert_eq!(
            w,
            Warning::UnsupportedGlob {
                pattern: "src/**/*.py".to_string()
            }
        );
    }

    #[test]
    fn unmodeled_run_command_stores_command_text() {
        let w = Warning::UnmodeledRunCommand {
            command: "curl https://example.com | bash".to_string(),
            reason: UnmodeledReason::UnrecognisedCommand,
        };
        assert_eq!(
            w,
            Warning::UnmodeledRunCommand {
                command: "curl https://example.com | bash".to_string(),
                reason: UnmodeledReason::UnrecognisedCommand,
            }
        );
    }

    #[test]
    fn simulated_install_stores_manager_and_packages() {
        let w = Warning::SimulatedInstall {
            manager: "apt".to_string(),
            packages: vec!["curl".to_string(), "wget".to_string()],
        };
        assert_eq!(
            w,
            Warning::SimulatedInstall {
                manager: "apt".to_string(),
                packages: vec!["curl".to_string(), "wget".to_string()]
            }
        );
    }

    #[test]
    fn empty_base_image_stores_image_name() {
        let w = Warning::EmptyBaseImage {
            image: "scratch".to_string(),
        };
        assert_eq!(
            w,
            Warning::EmptyBaseImage {
                image: "scratch".to_string()
            }
        );
    }

    // --- Display output ---

    #[test]
    fn display_unsupported_instruction_is_non_empty_and_contains_name() {
        let w = Warning::UnsupportedInstruction {
            instruction: "HEALTHCHECK".to_string(),
            line: 5,
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(s.contains("HEALTHCHECK"), "must contain instruction name");
        // Line number must appear so users can locate the instruction in their Dockerfile.
        assert!(s.contains('5'), "must contain line number 5, got: {s}");
    }

    #[test]
    fn display_missing_copy_source_contains_path() {
        let w = Warning::MissingCopySource {
            path: PathBuf::from("/app/missing.txt"),
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(s.contains("missing.txt"));
    }

    #[test]
    fn display_unsupported_glob_contains_pattern() {
        let w = Warning::UnsupportedGlob {
            pattern: "**/*.rs".to_string(),
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(s.contains("**/*.rs"));
    }

    #[test]
    fn display_unmodeled_run_command_contains_command() {
        let w = Warning::UnmodeledRunCommand {
            command: "make install".to_string(),
            reason: UnmodeledReason::UnrecognisedCommand,
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(s.contains("make install"));
    }

    #[test]
    fn display_simulated_install_contains_manager_and_packages() {
        let w = Warning::SimulatedInstall {
            manager: "pip".to_string(),
            packages: vec!["requests".to_string()],
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(s.contains("pip"));
        assert!(s.contains("requests"));
    }

    #[test]
    fn display_empty_base_image_contains_image_name() {
        let w = Warning::EmptyBaseImage {
            image: "scratch".to_string(),
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(s.contains("scratch"));
    }

    // --- UnmodeledReason construction ---

    #[test]
    fn unmodeled_reason_unrecognised_command_constructs() {
        let r = UnmodeledReason::UnrecognisedCommand;
        assert_eq!(r, UnmodeledReason::UnrecognisedCommand);
    }

    #[test]
    fn unmodeled_reason_unsupported_flag_stores_flag() {
        let r = UnmodeledReason::UnsupportedFlag {
            flag: "--dry-run".to_string(),
        };
        assert_eq!(
            r,
            UnmodeledReason::UnsupportedFlag {
                flag: "--dry-run".to_string()
            }
        );
    }

    #[test]
    fn unmodeled_reason_unsupported_usage_constructs() {
        let r = UnmodeledReason::UnsupportedUsage;
        assert_eq!(r, UnmodeledReason::UnsupportedUsage);
    }

    #[test]
    fn unmodeled_reason_variants_are_not_equal_to_each_other() {
        assert_ne!(
            UnmodeledReason::UnrecognisedCommand,
            UnmodeledReason::UnsupportedUsage
        );
        assert_ne!(
            UnmodeledReason::UnrecognisedCommand,
            UnmodeledReason::UnsupportedFlag {
                flag: "-s".to_string()
            }
        );
    }

    // --- UnmodeledReason Display ---

    #[test]
    fn display_unrecognised_command_contains_first_word_of_command() {
        // The Display for the whole Warning should contain the first word of the command.
        let w = Warning::UnmodeledRunCommand {
            command: "curl https://example.com".to_string(),
            reason: UnmodeledReason::UnrecognisedCommand,
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(
            s.contains("curl"),
            "display must contain first word 'curl', got: {s}"
        );
        assert!(
            s.contains("curl https://example.com"),
            "display must contain full command, got: {s}"
        );
    }

    #[test]
    fn display_unsupported_flag_contains_flag_name() {
        let w = Warning::UnmodeledRunCommand {
            command: "apt-get install --dry-run curl".to_string(),
            reason: UnmodeledReason::UnsupportedFlag {
                flag: "--dry-run".to_string(),
            },
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(
            s.contains("--dry-run"),
            "display must contain flag name '--dry-run', got: {s}"
        );
        assert!(
            s.contains("apt-get install --dry-run curl"),
            "display must contain full command, got: {s}"
        );
    }

    #[test]
    fn display_unsupported_usage_contains_command() {
        let w = Warning::UnmodeledRunCommand {
            command: "cp /a /b /c".to_string(),
            reason: UnmodeledReason::UnsupportedUsage,
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(
            s.contains("cp /a /b /c"),
            "display must contain command, got: {s}"
        );
        assert!(
            s.contains("unsupported usage"),
            "display must contain 'unsupported usage', got: {s}"
        );
    }

    #[test]
    fn display_unrecognised_command_contains_not_modeled() {
        let w = Warning::UnmodeledRunCommand {
            command: "make build".to_string(),
            reason: UnmodeledReason::UnrecognisedCommand,
        };
        let s = w.to_string();
        assert!(
            s.contains("not modeled: make"),
            "display must contain 'not modeled: make', got: {s}"
        );
    }

    // --- Full-string Display format assertions ---
    // These lock the exact user-visible output format against silent drift.

    #[test]
    fn display_unrecognised_command_full_string() {
        let w = Warning::UnmodeledRunCommand {
            command: "echo hello".to_string(),
            reason: UnmodeledReason::UnrecognisedCommand,
        };
        assert_eq!(
            w.to_string(),
            "skipped RUN sub-command 'echo hello' (not modeled: echo)"
        );
    }

    #[test]
    fn display_unsupported_flag_full_string() {
        let w = Warning::UnmodeledRunCommand {
            command: "apt-get install --dry-run curl".to_string(),
            reason: UnmodeledReason::UnsupportedFlag {
                flag: "--dry-run".to_string(),
            },
        };
        assert_eq!(
            w.to_string(),
            "skipped RUN sub-command 'apt-get install --dry-run curl' (unsupported flag: --dry-run)"
        );
    }

    #[test]
    fn display_unsupported_usage_full_string() {
        let w = Warning::UnmodeledRunCommand {
            command: "cp /a /b /c".to_string(),
            reason: UnmodeledReason::UnsupportedUsage,
        };
        assert_eq!(
            w.to_string(),
            "skipped RUN sub-command 'cp /a /b /c' (unsupported usage)"
        );
    }

    // --- Clone and PartialEq ---

    #[test]
    fn warning_clone_produces_equal_value() {
        let w = Warning::SimulatedInstall {
            manager: "apt".to_string(),
            packages: vec!["git".to_string()],
        };
        assert_eq!(w.clone(), w);
    }

    #[test]
    fn different_warning_variants_are_not_equal() {
        let a = Warning::UnsupportedInstruction {
            instruction: "X".to_string(),
            line: 1,
        };
        let b = Warning::EmptyBaseImage {
            image: "X".to_string(),
        };
        assert_ne!(a, b);
    }

    // --- MissingCopyStage ---

    #[test]
    fn missing_copy_stage_stores_stage_and_line() {
        let w = Warning::MissingCopyStage {
            stage: "builder".to_string(),
            line: 7,
        };
        assert_eq!(
            w,
            Warning::MissingCopyStage {
                stage: "builder".to_string(),
                line: 7
            }
        );
    }

    #[test]
    fn display_missing_copy_stage_contains_stage_name() {
        let w = Warning::MissingCopyStage {
            stage: "builder".to_string(),
            line: 7,
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(
            s.contains("builder"),
            "display must contain stage name, got: {s}"
        );
        assert!(
            s.contains('7'),
            "display must contain line number 7, got: {s}"
        );
    }

    #[test]
    fn display_missing_copy_stage_full_string() {
        let w = Warning::MissingCopyStage {
            stage: "mybuilder".to_string(),
            line: 12,
        };
        assert_eq!(
            w.to_string(),
            "COPY --from='mybuilder' references unknown stage at line 12 (skipped)"
        );
    }

    #[test]
    fn display_missing_copy_stage_numeric_index() {
        // Numeric stage indices (e.g. "0") must appear in the message.
        let w = Warning::MissingCopyStage {
            stage: "0".to_string(),
            line: 3,
        };
        let s = w.to_string();
        assert!(
            s.contains("'0'"),
            "display must contain quoted stage '0', got: {s}"
        );
    }

    #[test]
    fn missing_copy_stage_clone_produces_equal_value() {
        let w = Warning::MissingCopyStage {
            stage: "runner".to_string(),
            line: 5,
        };
        assert_eq!(w.clone(), w);
    }

    // --- ImageOnlyService ---

    #[test]
    fn image_only_service_stores_image() {
        let w = Warning::ImageOnlyService {
            image: "postgres:15".to_string(),
        };
        assert_eq!(
            w,
            Warning::ImageOnlyService {
                image: "postgres:15".to_string()
            }
        );
    }

    #[test]
    fn display_image_only_service_contains_image_name() {
        let w = Warning::ImageOnlyService {
            image: "postgres:15".to_string(),
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(
            s.contains("postgres:15"),
            "display must contain image name, got: {s}"
        );
        assert!(
            s.contains("filesystem is empty"),
            "display must mention empty filesystem, got: {s}"
        );
    }

    #[test]
    fn display_image_only_service_full_string() {
        let w = Warning::ImageOnlyService {
            image: "redis:7".to_string(),
        };
        assert_eq!(
            w.to_string(),
            "service uses image 'redis:7' with no build context \u{2014} filesystem is empty"
        );
    }

    // --- EnvFileNotFound ---

    #[test]
    fn env_file_not_found_stores_path() {
        let path = PathBuf::from("/project/.env.prod");
        let w = Warning::EnvFileNotFound { path: path.clone() };
        assert_eq!(w, Warning::EnvFileNotFound { path });
    }

    #[test]
    fn display_env_file_not_found_contains_path() {
        let w = Warning::EnvFileNotFound {
            path: PathBuf::from("/project/.env"),
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(s.contains(".env"), "display must contain path, got: {s}");
        assert!(
            s.contains("not found"),
            "display must say 'not found', got: {s}"
        );
    }

    #[test]
    fn display_env_file_not_found_full_string() {
        let w = Warning::EnvFileNotFound {
            path: PathBuf::from(".env"),
        };
        assert_eq!(w.to_string(), "env_file '.env' not found \u{2014} skipped");
    }

    // --- UnresolvedEnvKey ---

    #[test]
    fn unresolved_env_key_stores_key() {
        let w = Warning::UnresolvedEnvKey {
            key: "HOST_TOKEN".to_string(),
        };
        assert_eq!(
            w,
            Warning::UnresolvedEnvKey {
                key: "HOST_TOKEN".to_string()
            }
        );
    }

    #[test]
    fn display_unresolved_env_key_contains_key_name() {
        let w = Warning::UnresolvedEnvKey {
            key: "SECRET".to_string(),
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(
            s.contains("SECRET"),
            "display must contain key name, got: {s}"
        );
        assert!(
            s.contains("no value"),
            "display must mention 'no value', got: {s}"
        );
    }

    #[test]
    fn display_unresolved_env_key_full_string() {
        let w = Warning::UnresolvedEnvKey {
            key: "MY_VAR".to_string(),
        };
        assert_eq!(
            w.to_string(),
            "environment key 'MY_VAR' has no value \u{2014} skipped"
        );
    }

    #[test]
    fn new_warning_variants_clone_and_equal() {
        let w1 = Warning::ImageOnlyService {
            image: "img".to_string(),
        };
        let w2 = Warning::EnvFileNotFound {
            path: PathBuf::from("/a"),
        };
        let w3 = Warning::UnresolvedEnvKey {
            key: "K".to_string(),
        };
        assert_eq!(w1.clone(), w1);
        assert_eq!(w2.clone(), w2);
        assert_eq!(w3.clone(), w3);
        assert_ne!(w1, w3);
    }

    // --- WorkdirEscapedRoot ---

    #[test]
    fn workdir_escaped_root_stores_path() {
        let w = Warning::WorkdirEscapedRoot {
            path: PathBuf::from("../../../etc"),
        };
        assert_eq!(
            w,
            Warning::WorkdirEscapedRoot {
                path: PathBuf::from("../../../etc")
            }
        );
    }

    #[test]
    fn display_workdir_escaped_root_contains_path_and_clamped() {
        let w = Warning::WorkdirEscapedRoot {
            path: PathBuf::from("../../../etc"),
        };
        let s = w.to_string();
        assert!(!s.is_empty());
        assert!(
            s.contains("../../../etc"),
            "must contain original path; got: {s}"
        );
        assert!(s.contains("clamped"), "must mention clamping; got: {s}");
    }

    #[test]
    fn workdir_escaped_root_clone_produces_equal_value() {
        let w = Warning::WorkdirEscapedRoot {
            path: PathBuf::from("../../bad"),
        };
        assert_eq!(w.clone(), w);
    }
}
