// Main engine runner that applies a sequence of instructions to a PreviewState.
//
// The single public entry point `run` walks a `Vec<Instruction>` and returns
// a fully-populated `PreviewState`. Each instruction produces a `LayerSummary`
// and a `HistoryEntry` that describe what changed.
//
// Recoverable conditions (missing COPY sources, unmodeled RUN commands,
// unsupported instructions) become `Warning`s in `PreviewState`. Only
// unrecoverable I/O errors propagate as `EngineError`.

use std::path::Path;

use crate::model::{
    fs::{DirNode, FsNode},
    instruction::Instruction,
    provenance::{Provenance, ProvenanceSource},
    state::{HistoryEntry, LayerSummary, PreviewState},
    warning::Warning,
};

use super::{copy, EngineError};

/// Run a sequence of Dockerfile instructions against a fresh `PreviewState`.
///
/// # Parameters
/// - `instructions` — the parsed instruction list to execute in order
/// - `context_dir`  — the host build context directory for COPY operations
///
/// # Returns
/// The populated `PreviewState`, or an `EngineError` for unrecoverable I/O
/// failures.
pub fn run(
    instructions: Vec<Instruction>,
    context_dir: &Path,
) -> Result<PreviewState, EngineError> {
    let mut state = PreviewState::default();

    for (idx, instruction) in instructions.into_iter().enumerate() {
        // Use index+1 as a line proxy; real line numbers are a follow-up task.
        let line = idx + 1;

        let (raw_text, effect, layer) = match &instruction {
            Instruction::From { image, alias } => {
                // FROM establishes the base image. Since we do not model image
                // content yet, we emit an EmptyBaseImage warning and set the
                // active stage alias if present.
                state.warnings.push(Warning::EmptyBaseImage {
                    image: image.clone(),
                });
                state.active_stage = alias.clone();

                let effect = format!("base image '{}' (empty — not modeled)", image);
                let layer = LayerSummary {
                    instruction_type: "FROM".to_string(),
                    files_changed: vec![],
                    env_changed: vec![],
                };
                let raw = match alias {
                    Some(a) => format!("FROM {} AS {}", image, a),
                    None => format!("FROM {}", image),
                };
                (raw, effect, layer)
            }

            Instruction::Workdir { path } => {
                let layer = apply_workdir(&mut state, path, line);
                let effect = format!("set cwd to {}", state.cwd.display());
                let raw = format!("WORKDIR {}", path.display());
                (raw, effect, layer)
            }

            Instruction::Copy { source, dest } => {
                let raw = format!("COPY {:?} {}", source, dest.display());
                let layer = copy::handle_copy(&mut state, source, dest, context_dir, line)?;
                let effect = format!("{} file(s) copied", layer.files_changed.len());
                (raw, effect, layer)
            }

            Instruction::Env { key, value } => {
                state.env.insert(key.clone(), value.clone());
                let effect = format!("set {}={}", key, value);
                let layer = LayerSummary {
                    instruction_type: "ENV".to_string(),
                    files_changed: vec![],
                    env_changed: vec![(key.clone(), value.clone())],
                };
                let raw = format!("ENV {}={}", key, value);
                (raw, effect, layer)
            }

            Instruction::Run { command } => {
                let layer = super::run_sim::handle_run(&mut state, command, line);
                let effect = format!("RUN (unmodeled): {}", command);
                let raw = format!("RUN {}", command);
                (raw, effect, layer)
            }

            Instruction::Unknown { raw } => {
                // Extract the instruction keyword for the warning (first word).
                let keyword = raw
                    .split_whitespace()
                    .next()
                    .unwrap_or("UNKNOWN")
                    .to_uppercase();
                state.warnings.push(Warning::UnsupportedInstruction {
                    instruction: keyword.clone(),
                    line,
                });
                let effect = format!("unsupported instruction '{}' (skipped)", keyword);
                let layer = LayerSummary {
                    instruction_type: keyword,
                    files_changed: vec![],
                    env_changed: vec![],
                };
                (raw.clone(), effect, layer)
            }
        };

        // Record history entry and layer summary for every instruction.
        state.history.push(HistoryEntry {
            line,
            instruction: raw_text,
            effect,
        });
        state.layers.push(layer);
    }

    Ok(state)
}

/// Apply a `WORKDIR` instruction: update `state.cwd` and ensure the directory
/// and its ancestors exist in the virtual filesystem.
///
/// - Absolute paths replace `cwd` entirely.
/// - Relative paths are joined onto the current `cwd`.
fn apply_workdir(state: &mut PreviewState, path: &Path, _line: usize) -> LayerSummary {
    let new_cwd = if path.is_absolute() {
        path.to_path_buf()
    } else {
        state.cwd.join(path)
    };

    // Insert directory nodes for the new cwd and all its ancestors.
    let provenance = ProvenanceSource::Workdir;
    copy::ensure_ancestors(&mut state.fs, &new_cwd, provenance.clone());

    // Insert the cwd directory itself if it doesn't exist yet.
    if !state.fs.contains(&new_cwd) {
        state.fs.insert(
            new_cwd.clone(),
            FsNode::Directory(DirNode {
                provenance: Provenance::new(provenance),
                permissions: None,
            }),
        );
    }

    state.cwd = new_cwd.clone();

    LayerSummary {
        instruction_type: "WORKDIR".to_string(),
        files_changed: vec![new_cwd],
        env_changed: vec![],
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::model::{
        fs::FsNode,
        instruction::{CopySource, Instruction},
        warning::Warning,
    };
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    // ── helpers ───────────────────────────────────────────────────────────

    /// Create a temp directory with a single file.
    fn make_context(filename: &str, content: &[u8]) -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(filename), content).expect("write");
        dir
    }

    /// Empty context dir for instructions that don't need real files.
    fn empty_context() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    // ── test 1: empty instructions returns default state ──────────────────

    #[test]
    fn empty_instructions_returns_default_state() {
        let ctx = empty_context();
        let state = run(vec![], ctx.path()).expect("run");

        assert_eq!(state.cwd, PathBuf::from("/"));
        assert!(state.env.is_empty());
        assert!(state.history.is_empty());
        assert!(state.layers.is_empty());
        assert!(state.warnings.is_empty());
    }

    // ── test 2: FROM with alias sets active_stage and emits warning ───────

    #[test]
    fn from_sets_active_stage_and_emits_warning() {
        let ctx = empty_context();
        let state = run(
            vec![Instruction::From {
                image: "ubuntu:22.04".to_string(),
                alias: Some("builder".to_string()),
            }],
            ctx.path(),
        )
        .expect("run");

        assert_eq!(state.active_stage, Some("builder".to_string()));
        assert!(
            state
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::EmptyBaseImage { .. })),
            "expected EmptyBaseImage warning"
        );
    }

    // ── test 3: FROM without alias leaves active_stage as None ───────────

    #[test]
    fn from_without_alias_sets_active_stage_none() {
        let ctx = empty_context();
        let state = run(
            vec![Instruction::From {
                image: "scratch".to_string(),
                alias: None,
            }],
            ctx.path(),
        )
        .expect("run");

        assert!(state.active_stage.is_none());
    }

    // ── test 4: absolute WORKDIR updates cwd and creates dir ──────────────

    #[test]
    fn workdir_absolute_updates_cwd_and_creates_dir() {
        let ctx = empty_context();
        let state = run(
            vec![Instruction::Workdir {
                path: PathBuf::from("/app"),
            }],
            ctx.path(),
        )
        .expect("run");

        assert_eq!(state.cwd, PathBuf::from("/app"));
        assert!(
            state.fs.contains(Path::new("/app")),
            "/app must exist in fs"
        );
    }

    // ── test 5: relative WORKDIR resolves against cwd ────────────────────

    #[test]
    fn workdir_relative_resolves_against_cwd() {
        let ctx = empty_context();
        let state = run(
            vec![
                Instruction::Workdir {
                    path: PathBuf::from("/opt"),
                },
                Instruction::Workdir {
                    path: PathBuf::from("sub"),
                },
            ],
            ctx.path(),
        )
        .expect("run");

        assert_eq!(state.cwd, PathBuf::from("/opt/sub"));
    }

    // ── test 6: WORKDIR creates ancestor dirs ─────────────────────────────

    #[test]
    fn workdir_creates_ancestor_dirs() {
        let ctx = empty_context();
        let state = run(
            vec![Instruction::Workdir {
                path: PathBuf::from("/a/b/c"),
            }],
            ctx.path(),
        )
        .expect("run");

        assert!(state.fs.contains(Path::new("/a")), "/a must exist");
        assert!(state.fs.contains(Path::new("/a/b")), "/a/b must exist");
        assert!(state.fs.contains(Path::new("/a/b/c")), "/a/b/c must exist");
    }

    // ── test 7: ENV inserts into state.env ───────────────────────────────

    #[test]
    fn env_inserts_into_state_env() {
        let ctx = empty_context();
        let state = run(
            vec![Instruction::Env {
                key: "KEY".to_string(),
                value: "value".to_string(),
            }],
            ctx.path(),
        )
        .expect("run");

        assert_eq!(state.env.get("KEY"), Some(&"value".to_string()));
    }

    // ── test 8: ENV layer summary records change ──────────────────────────

    #[test]
    fn env_layer_summary_records_change() {
        let ctx = empty_context();
        let state = run(
            vec![Instruction::Env {
                key: "KEY".to_string(),
                value: "value".to_string(),
            }],
            ctx.path(),
        )
        .expect("run");

        let layer = &state.layers[0];
        assert!(
            layer.env_changed.contains(&("KEY".to_string(), "value".to_string())),
            "env_changed must contain (KEY, value)"
        );
    }

    // ── test 9: RUN instruction emits warning ─────────────────────────────

    #[test]
    fn run_instruction_emits_warning() {
        let ctx = empty_context();
        let state = run(
            vec![Instruction::Run {
                command: "echo done".to_string(),
            }],
            ctx.path(),
        )
        .expect("run");

        assert!(
            state
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. })),
            "expected UnmodeledRunCommand warning"
        );
    }

    // ── test 10: Unknown instruction emits UnsupportedInstruction warning ─

    #[test]
    fn unknown_instruction_emits_warning() {
        let ctx = empty_context();
        let state = run(
            vec![Instruction::Unknown {
                raw: "EXPOSE 8080".to_string(),
            }],
            ctx.path(),
        )
        .expect("run");

        assert!(
            state
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::UnsupportedInstruction { .. })),
            "expected UnsupportedInstruction warning"
        );
    }

    // ── test 11: every instruction produces a history entry ───────────────

    #[test]
    fn every_instruction_produces_history_entry() {
        let ctx = empty_context();
        let instrs = vec![
            Instruction::From {
                image: "scratch".to_string(),
                alias: None,
            },
            Instruction::Workdir {
                path: PathBuf::from("/app"),
            },
            Instruction::Env {
                key: "A".to_string(),
                value: "1".to_string(),
            },
            Instruction::Run {
                command: "true".to_string(),
            },
        ];
        let state = run(instrs, ctx.path()).expect("run");
        assert_eq!(state.history.len(), 4);
    }

    // ── test 12: every instruction produces a layer summary ──────────────

    #[test]
    fn every_instruction_produces_layer_summary() {
        let ctx = empty_context();
        let instrs = vec![
            Instruction::From {
                image: "scratch".to_string(),
                alias: None,
            },
            Instruction::Workdir {
                path: PathBuf::from("/app"),
            },
            Instruction::Env {
                key: "A".to_string(),
                value: "1".to_string(),
            },
            Instruction::Run {
                command: "true".to_string(),
            },
        ];
        let state = run(instrs, ctx.path()).expect("run");
        assert_eq!(state.layers.len(), 4);
    }

    // ── test 13: multiple ENVs accumulate ────────────────────────────────

    #[test]
    fn multiple_envs_accumulate() {
        let ctx = empty_context();
        let state = run(
            vec![
                Instruction::Env {
                    key: "A".to_string(),
                    value: "1".to_string(),
                },
                Instruction::Env {
                    key: "B".to_string(),
                    value: "2".to_string(),
                },
                Instruction::Env {
                    key: "C".to_string(),
                    value: "3".to_string(),
                },
            ],
            ctx.path(),
        )
        .expect("run");

        assert_eq!(state.env.get("A"), Some(&"1".to_string()));
        assert_eq!(state.env.get("B"), Some(&"2".to_string()));
        assert_eq!(state.env.get("C"), Some(&"3".to_string()));
    }

    // ── test 14: COPY reads real file from context ────────────────────────

    #[test]
    fn copy_instruction_reads_real_file() {
        let ctx = make_context("hello.txt", b"hello world");
        let state = run(
            vec![Instruction::Copy {
                source: CopySource::Host(PathBuf::from("hello.txt")),
                dest: PathBuf::from("/app/hello.txt"),
            }],
            ctx.path(),
        )
        .expect("run");

        let node = state.fs.get(Path::new("/app/hello.txt")).expect("file");
        match node {
            FsNode::File(f) => assert_eq!(f.content, b"hello world"),
            _ => panic!("expected File"),
        }
    }

    // ── test 15: WORKDIR then COPY with relative dest ─────────────────────

    #[test]
    fn workdir_then_copy_relative_dest() {
        let ctx = make_context("data.txt", b"data");
        let state = run(
            vec![
                Instruction::Workdir {
                    path: PathBuf::from("/app"),
                },
                Instruction::Copy {
                    source: CopySource::Host(PathBuf::from("data.txt")),
                    dest: PathBuf::from("output/data.txt"),
                },
            ],
            ctx.path(),
        )
        .expect("run");

        assert!(
            state.fs.contains(Path::new("/app/output/data.txt")),
            "relative COPY dest must resolve against cwd"
        );
    }
}
