// Typed representation of the Dockerfile instructions that the engine handles.
//
// Instructions are produced by the parser and consumed by the engine.
// Only the v0.1 subset is modeled here; unsupported instructions are
// captured as `Instruction::Unknown` so the engine can record them
// in history and emit warnings without crashing.

use std::path::PathBuf;

/// Describes where a COPY instruction reads its source files from.
#[derive(Debug, Clone, PartialEq)]
pub enum CopySource {
    /// Source is a path on the host build context (v0.1 support).
    Host(PathBuf),

    /// Files copied from a named build stage via `COPY --from=<stage>`.
    ///
    /// `name` is the stage alias or numeric index string (e.g. `"builder"` or `"0"`).
    /// `src_path` is the absolute path inside the source stage's virtual filesystem
    /// that the engine will copy from when processing this instruction.
    Stage {
        /// The stage alias or numeric index string used in `--from=<name>`.
        ///
        /// Matched against `StageRegistry` keys: both aliases (e.g. `"builder"`)
        /// and numeric index strings (e.g. `"0"`) are valid.
        name: String,
        /// The path inside the source stage's virtual filesystem to copy from.
        ///
        /// Relative paths are treated as absolute from the stage root (i.e.
        /// a leading `/` is prepended if absent) when the engine resolves them.
        src_path: PathBuf,
    },
}

/// A single Dockerfile instruction, fully parsed and typed.
///
/// Unknown or unsupported instructions are stored as `Unknown { raw }` so
/// the engine can still record history and emit a warning without panicking.
#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    /// `FROM <image> [AS <alias>]`
    From {
        /// The base image name (e.g. "ubuntu:22.04", "alpine:3.18").
        image: String,
        /// Optional stage alias for multi-stage builds.
        alias: Option<String>,
    },

    /// `WORKDIR <path>`
    Workdir {
        /// The target working directory path.
        path: PathBuf,
    },

    /// `COPY <source> <dest>`
    Copy {
        /// Where the source files come from.
        source: CopySource,
        /// The destination path inside the container filesystem.
        dest: PathBuf,
    },

    /// `ENV <key>=<value>`
    Env {
        /// The environment variable key.
        key: String,
        /// The environment variable value.
        value: String,
    },

    /// `RUN <command>`
    Run {
        /// The raw shell command string to simulate.
        command: String,
    },

    /// Any instruction the parser did not recognise or that is not yet supported.
    Unknown {
        /// The original raw line from the Dockerfile.
        raw: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- CopySource variants ---

    #[test]
    fn copy_source_host_stores_path() {
        let path = PathBuf::from("./src");
        let src = CopySource::Host(path.clone());
        assert_eq!(src, CopySource::Host(path));
    }

    #[test]
    fn copy_source_stage_stores_name_and_src_path() {
        let src = CopySource::Stage {
            name: "builder".to_string(),
            src_path: PathBuf::from("/out/app"),
        };
        assert_eq!(
            src,
            CopySource::Stage {
                name: "builder".to_string(),
                src_path: PathBuf::from("/out/app"),
            }
        );
    }

    #[test]
    fn copy_source_host_and_stage_are_not_equal() {
        let host = CopySource::Host(PathBuf::from("builder"));
        let stage = CopySource::Stage {
            name: "builder".to_string(),
            src_path: PathBuf::from("/out/app"),
        };
        assert_ne!(host, stage);
    }

    // --- Instruction::From ---

    #[test]
    fn instruction_from_without_alias() {
        let inst = Instruction::From {
            image: "ubuntu:22.04".to_string(),
            alias: None,
        };
        assert_eq!(
            inst,
            Instruction::From {
                image: "ubuntu:22.04".to_string(),
                alias: None
            }
        );
    }

    #[test]
    fn instruction_from_with_alias() {
        let inst = Instruction::From {
            image: "rust:1.75".to_string(),
            alias: Some("builder".to_string()),
        };
        assert_eq!(
            inst,
            Instruction::From {
                image: "rust:1.75".to_string(),
                alias: Some("builder".to_string())
            }
        );
    }

    // --- Instruction::Workdir ---

    #[test]
    fn instruction_workdir_stores_path() {
        let inst = Instruction::Workdir {
            path: PathBuf::from("/app"),
        };
        assert_eq!(
            inst,
            Instruction::Workdir {
                path: PathBuf::from("/app")
            }
        );
    }

    // --- Instruction::Copy ---

    #[test]
    fn instruction_copy_with_host_source() {
        let inst = Instruction::Copy {
            source: CopySource::Host(PathBuf::from(".")),
            dest: PathBuf::from("/app"),
        };
        assert_eq!(
            inst,
            Instruction::Copy {
                source: CopySource::Host(PathBuf::from(".")),
                dest: PathBuf::from("/app")
            }
        );
    }

    #[test]
    fn instruction_copy_with_stage_source() {
        let inst = Instruction::Copy {
            source: CopySource::Stage {
                name: "builder".to_string(),
                src_path: PathBuf::from("/out/app"),
            },
            dest: PathBuf::from("/app/binary"),
        };
        assert_eq!(
            inst,
            Instruction::Copy {
                source: CopySource::Stage {
                    name: "builder".to_string(),
                    src_path: PathBuf::from("/out/app"),
                },
                dest: PathBuf::from("/app/binary")
            }
        );
    }

    // --- Instruction::Env ---

    #[test]
    fn instruction_env_stores_key_and_value() {
        let inst = Instruction::Env {
            key: "NODE_ENV".to_string(),
            value: "production".to_string(),
        };
        assert_eq!(
            inst,
            Instruction::Env {
                key: "NODE_ENV".to_string(),
                value: "production".to_string()
            }
        );
    }

    // --- Instruction::Run ---

    #[test]
    fn instruction_run_stores_command_text() {
        let inst = Instruction::Run {
            command: "apt-get install -y curl".to_string(),
        };
        assert_eq!(
            inst,
            Instruction::Run {
                command: "apt-get install -y curl".to_string()
            }
        );
    }

    // --- Instruction::Unknown ---

    #[test]
    fn instruction_unknown_stores_raw_text() {
        let inst = Instruction::Unknown {
            raw: "HEALTHCHECK CMD curl http://localhost".to_string(),
        };
        assert_eq!(
            inst,
            Instruction::Unknown {
                raw: "HEALTHCHECK CMD curl http://localhost".to_string()
            }
        );
    }

    // --- Clone ---

    #[test]
    fn instruction_clone_produces_equal_value() {
        let inst = Instruction::Run {
            command: "echo hello".to_string(),
        };
        assert_eq!(inst.clone(), inst);
    }

    // --- Inequality across variants ---

    #[test]
    fn different_instruction_variants_are_not_equal() {
        let run = Instruction::Run {
            command: "echo hi".to_string(),
        };
        let unknown = Instruction::Unknown {
            raw: "echo hi".to_string(),
        };
        assert_ne!(run, unknown);
    }
}
