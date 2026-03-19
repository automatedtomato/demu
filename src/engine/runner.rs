// Main engine runner that applies a sequence of instructions to a PreviewState.
//
// The single public entry point `run` walks a `Vec<Instruction>` and returns
// an `EngineOutput` that contains:
//   - the final stage's `PreviewState` (ready for the REPL)
//   - a `StageRegistry` with all completed stages indexed by numeric index and alias
//
// Multi-stage builds (multiple FROM instructions) are handled by saving the
// current state into the registry each time a new FROM is encountered. The final
// stage is inserted into the registry after the loop and also returned as `state`.
//
// Recoverable conditions (missing COPY sources, unmodeled RUN commands,
// unsupported instructions) become `Warning`s in `PreviewState`. Only
// unrecoverable I/O errors propagate as `EngineError`.

use std::path::Path;

use crate::model::{
    fs::{DirNode, FsNode},
    instruction::Instruction,
    provenance::{Provenance, ProvenanceSource},
    state::{HistoryEntry, LayerSummary, PreviewState, StageRegistry},
    warning::Warning,
};

use super::{copy, EngineError};

/// The output produced by a full engine run over a Dockerfile instruction list.
///
/// `state` is the final stage's `PreviewState`, which is the default target for
/// the REPL. `stages` is the `StageRegistry` that maps every stage (by numeric
/// index and optional alias) to its `PreviewState`, enabling `--stage` selection.
pub struct EngineOutput {
    /// The final (last) stage's `PreviewState`, ready for the REPL.
    pub state: PreviewState,
    /// All completed stages, indexed by numeric index string and by alias (if any).
    pub stages: StageRegistry,
}

/// Run a sequence of Dockerfile instructions against a fresh `PreviewState`.
///
/// Supports multi-stage builds: each `FROM` instruction after the first saves
/// the current stage into the `StageRegistry` before starting a new one.
///
/// # Parameters
/// - `instructions` — the parsed instruction list to execute in order
/// - `context_dir`  — the host build context directory for COPY operations
///
/// # Returns
/// An `EngineOutput` containing the final stage state and the full registry,
/// or an `EngineError` for unrecoverable I/O failures.
pub fn run(
    instructions: Vec<Instruction>,
    context_dir: &Path,
) -> Result<EngineOutput, EngineError> {
    let mut state = PreviewState::default();
    let mut registry = StageRegistry::default();
    // Tracks how many FROM instructions have been processed (0-based stage index).
    let mut stage_index: usize = 0;
    // Current stage alias, updated on each FROM … AS <alias> instruction.
    let mut current_alias: Option<String> = None;

    for (idx, instruction) in instructions.into_iter().enumerate() {
        // Use index+1 as a line proxy; real line numbers are a follow-up task.
        let line = idx + 1;

        let (raw_text, effect, layer) = match &instruction {
            Instruction::From { image, alias } => {
                // Multi-stage support: if a stage is already in progress (either
                // this is not the first FROM, or the current state has history),
                // save it to the registry before resetting for the new stage.
                if stage_index > 0 || !state.history.is_empty() {
                    registry.insert(stage_index, current_alias.as_deref(), state);
                    stage_index += 1;
                    state = PreviewState::default();
                }
                // Update alias tracking for the new stage.
                current_alias = alias.clone();

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
                let layer =
                    copy::handle_copy(&mut state, source, dest, context_dir, line, &registry)?;
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

    // Insert the final stage into the registry so `--stage <index>` and
    // `--stage <alias>` can select it. The final state is also returned
    // directly as `EngineOutput::state` for the default REPL path.
    registry.insert(stage_index, current_alias.as_deref(), state.clone());

    Ok(EngineOutput {
        state,
        stages: registry,
    })
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
        let state = run(vec![], ctx.path()).expect("run").state;

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
        .expect("run")
        .state;

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
        .expect("run")
        .state;

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
        .expect("run")
        .state;

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
        .expect("run")
        .state;

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
        .expect("run")
        .state;

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
        .expect("run")
        .state;

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
        .expect("run")
        .state;

        let layer = &state.layers[0];
        assert!(
            layer
                .env_changed
                .contains(&("KEY".to_string(), "value".to_string())),
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
        .expect("run")
        .state;

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
        .expect("run")
        .state;

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
        let state = run(instrs, ctx.path()).expect("run").state;
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
        let state = run(instrs, ctx.path()).expect("run").state;
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
        .expect("run")
        .state;

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
        .expect("run")
        .state;

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
        .expect("run")
        .state;

        assert!(
            state.fs.contains(Path::new("/app/output/data.txt")),
            "relative COPY dest must resolve against cwd"
        );
    }

    // ── multi-stage tests ─────────────────────────────────────────────────

    // ── test 16: single FROM produces one stage in registry ──────────────

    #[test]
    fn single_from_produces_one_stage_in_registry() {
        let ctx = empty_context();
        let output = run(
            vec![Instruction::From {
                image: "scratch".to_string(),
                alias: None,
            }],
            ctx.path(),
        )
        .expect("run");

        assert_eq!(output.stages.len(), 1, "one FROM → one stage in registry");
        assert!(
            output.stages.get("0").is_some(),
            "stage must be stored under index '0'"
        );
    }

    // ── test 17: two FROMs produce two stages ─────────────────────────────

    #[test]
    fn two_from_produces_two_stages() {
        let ctx = empty_context();
        let output = run(
            vec![
                Instruction::From {
                    image: "scratch".to_string(),
                    alias: None,
                },
                Instruction::From {
                    image: "alpine".to_string(),
                    alias: None,
                },
            ],
            ctx.path(),
        )
        .expect("run");

        assert_eq!(output.stages.len(), 2, "two FROMs → two stages");
        assert!(output.stages.get("0").is_some(), "stage '0' must exist");
        assert!(output.stages.get("1").is_some(), "stage '1' must exist");
    }

    // ── test 18: named stage stored by alias and index ────────────────────

    #[test]
    fn named_stage_stored_by_alias_and_index() {
        let ctx = empty_context();
        let output = run(
            vec![Instruction::From {
                image: "scratch".to_string(),
                alias: Some("builder".to_string()),
            }],
            ctx.path(),
        )
        .expect("run");

        // Must be accessible by both "0" and "builder".
        assert!(
            output.stages.get("0").is_some(),
            "named stage must be stored under numeric index '0'"
        );
        assert!(
            output.stages.get("builder").is_some(),
            "named stage must be stored under alias 'builder'"
        );
    }

    // ── test 19: unnamed stages stored by index only ──────────────────────

    #[test]
    fn unnamed_stages_stored_by_index_only() {
        let ctx = empty_context();
        let output = run(
            vec![
                Instruction::From {
                    image: "scratch".to_string(),
                    alias: None,
                },
                Instruction::From {
                    image: "alpine".to_string(),
                    alias: None,
                },
            ],
            ctx.path(),
        )
        .expect("run");

        let keys = output.stages.keys();
        // Only numeric keys should be present (no alias keys).
        assert!(keys.contains(&"0".to_string()), "key '0' must exist");
        assert!(keys.contains(&"1".to_string()), "key '1' must exist");
        assert_eq!(keys.len(), 2, "no alias keys should be present");
    }

    // ── test 20: each stage has independent filesystem ────────────────────
    //
    // Files added in stage 0 (via WORKDIR) must NOT appear in stage 1.

    #[test]
    fn each_stage_has_independent_fs() {
        let ctx = empty_context();
        let output = run(
            vec![
                Instruction::From {
                    image: "scratch".to_string(),
                    alias: None,
                },
                // Stage 0: create /stage0-dir
                Instruction::Workdir {
                    path: PathBuf::from("/stage0-dir"),
                },
                // Stage 1 starts here
                Instruction::From {
                    image: "alpine".to_string(),
                    alias: None,
                },
                // Stage 1: create /stage1-dir
                Instruction::Workdir {
                    path: PathBuf::from("/stage1-dir"),
                },
            ],
            ctx.path(),
        )
        .expect("run");

        let stage0 = output.stages.get("0").expect("stage 0");
        let stage1 = output.stages.get("1").expect("stage 1");

        // Stage 0 has /stage0-dir but not /stage1-dir.
        assert!(
            stage0.fs.contains(Path::new("/stage0-dir")),
            "stage 0 must contain /stage0-dir"
        );
        assert!(
            !stage0.fs.contains(Path::new("/stage1-dir")),
            "stage 0 must NOT contain /stage1-dir"
        );

        // Stage 1 has /stage1-dir but not /stage0-dir.
        assert!(
            stage1.fs.contains(Path::new("/stage1-dir")),
            "stage 1 must contain /stage1-dir"
        );
        assert!(
            !stage1.fs.contains(Path::new("/stage0-dir")),
            "stage 1 must NOT contain /stage0-dir"
        );
    }

    // ── test 21: each stage has independent env ───────────────────────────
    //
    // ENV vars set in stage 0 must not appear in stage 1's env.

    #[test]
    fn each_stage_has_independent_env() {
        let ctx = empty_context();
        let output = run(
            vec![
                Instruction::From {
                    image: "scratch".to_string(),
                    alias: None,
                },
                Instruction::Env {
                    key: "STAGE0_VAR".to_string(),
                    value: "hello".to_string(),
                },
                Instruction::From {
                    image: "alpine".to_string(),
                    alias: None,
                },
                Instruction::Env {
                    key: "STAGE1_VAR".to_string(),
                    value: "world".to_string(),
                },
            ],
            ctx.path(),
        )
        .expect("run");

        let stage0 = output.stages.get("0").expect("stage 0");
        let stage1 = output.stages.get("1").expect("stage 1");

        // Stage 0 has STAGE0_VAR, not STAGE1_VAR.
        assert_eq!(
            stage0.env.get("STAGE0_VAR").map(String::as_str),
            Some("hello"),
            "stage 0 must have STAGE0_VAR=hello"
        );
        assert!(
            !stage0.env.contains_key("STAGE1_VAR"),
            "stage 0 must NOT have STAGE1_VAR"
        );

        // Stage 1 has STAGE1_VAR, not STAGE0_VAR.
        assert_eq!(
            stage1.env.get("STAGE1_VAR").map(String::as_str),
            Some("world"),
            "stage 1 must have STAGE1_VAR=world"
        );
        assert!(
            !stage1.env.contains_key("STAGE0_VAR"),
            "stage 1 must NOT have STAGE0_VAR"
        );
    }

    // ── test 22: final stage state equals last stage in registry ─────────

    #[test]
    fn final_stage_state_equals_last_stage_in_registry() {
        let ctx = empty_context();
        let output = run(
            vec![
                Instruction::From {
                    image: "scratch".to_string(),
                    alias: None,
                },
                Instruction::From {
                    image: "alpine".to_string(),
                    alias: None,
                },
                Instruction::Env {
                    key: "FINAL".to_string(),
                    value: "yes".to_string(),
                },
            ],
            ctx.path(),
        )
        .expect("run");

        // The last stage (index 1) must match output.state.
        let last_in_registry = output.stages.get("1").expect("stage '1' must exist");
        assert_eq!(
            output.state.env.get("FINAL"),
            last_in_registry.env.get("FINAL"),
            "output.state env must match last stage in registry"
        );
        assert_eq!(
            output.state.history.len(),
            last_in_registry.history.len(),
            "output.state history length must match last stage in registry"
        );
    }
}
