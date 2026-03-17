// User-visible warnings emitted during Dockerfile simulation.
//
// Warnings are collected in `PreviewState` and surfaced via the REPL so the
// user understands where the simulation is approximate or incomplete.
// They are never fatal — the engine continues after recording a warning.

use std::fmt;
use std::path::PathBuf;

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
}

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
                write!(
                    f,
                    "glob pattern '{}' is not modeled; copy skipped",
                    pattern
                )
            }
            Warning::UnmodeledRunCommand { command } => {
                write!(
                    f,
                    "RUN command not fully modeled: '{}' (recorded in history, effects may be partial)",
                    command
                )
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
        };
        assert_eq!(
            w,
            Warning::UnmodeledRunCommand {
                command: "curl https://example.com | bash".to_string()
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
        assert!(s.contains("HEALTHCHECK"));
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
}
