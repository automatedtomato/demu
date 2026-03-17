//! End-to-end integration tests: parse fixture Dockerfiles, run through engine,
//! verify PreviewState.
//!
//! Each test loads a fixture Dockerfile from `tests/fixtures/engine/`, parses it
//! with `parse_dockerfile`, then runs it through `engine::run` against the shared
//! build context directory at `tests/fixtures/engine/context/`. Assertions cover
//! filesystem content, environment variables, warnings, and history/layer counts.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use demu::engine::run;
use demu::model::{fs::FsNode, warning::Warning};
use demu::parser::parse_dockerfile;
use std::path::Path;

/// Shared build context for all integration tests.
///
/// Using `CARGO_MANIFEST_DIR` ensures the path is correct regardless of
/// the working directory from which `cargo test` is invoked (e.g. in CI
/// the cwd may differ from the workspace root).
const CONTEXT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/engine/context");

// ── test: minimal full pipeline ───────────────────────────────────────────────

#[test]
fn test_minimal_full_pipeline() {
    let input = include_str!("fixtures/engine/minimal.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    // WORKDIR /app must set cwd.
    assert_eq!(state.cwd, std::path::PathBuf::from("/app"));

    // ENV GREETING=hi must be in env.
    assert_eq!(state.env.get("GREETING"), Some(&"hi".to_string()));

    // FROM + WORKDIR + COPY + ENV + RUN = 5 instructions.
    assert_eq!(state.history.len(), 5);
    assert_eq!(state.layers.len(), 5);

    // hello.txt must be readable with correct content.
    let node = state
        .fs
        .get(std::path::Path::new("/app/hello.txt"))
        .expect("file must exist");
    match node {
        FsNode::File(f) => {
            assert_eq!(f.content, b"hello from context");
        }
        _ => panic!("expected File node"),
    }

    // Warnings: EmptyBaseImage (FROM) + UnmodeledRunCommand (RUN).
    assert!(
        state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::EmptyBaseImage { .. })),
        "expected EmptyBaseImage warning"
    );
    assert!(
        state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. })),
        "expected UnmodeledRunCommand warning"
    );
}

// ── test: relative WORKDIR resolution ────────────────────────────────────────

#[test]
fn test_workdir_relative_resolution() {
    let input = include_str!("fixtures/engine/workdir_relative.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    // WORKDIR /opt then WORKDIR sub/dir → final cwd is /opt/sub/dir.
    assert_eq!(state.cwd, std::path::PathBuf::from("/opt/sub/dir"));

    // All intermediate directory nodes must exist.
    assert!(
        state.fs.contains(std::path::Path::new("/opt")),
        "/opt must exist"
    );
    assert!(
        state.fs.contains(std::path::Path::new("/opt/sub")),
        "/opt/sub must exist"
    );
    assert!(
        state.fs.contains(std::path::Path::new("/opt/sub/dir")),
        "/opt/sub/dir must exist"
    );
}

// ── test: missing COPY source emits warning ───────────────────────────────────

#[test]
fn test_copy_missing_source_emits_warning() {
    let input = include_str!("fixtures/engine/copy_missing.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    assert!(
        state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::MissingCopySource { .. })),
        "expected MissingCopySource warning"
    );
}

// ── test: directory COPY is recursive ────────────────────────────────────────

#[test]
fn test_copy_directory_recursive() {
    let input = include_str!("fixtures/engine/copy_directory.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    // `sub/nested.txt` from context must appear at `/data/nested.txt`.
    assert!(
        state.fs.contains(std::path::Path::new("/data/nested.txt")),
        "/data/nested.txt must exist after directory COPY"
    );
}

// ── test: unknown instructions emit warnings ──────────────────────────────────

#[test]
fn test_unknown_instructions_emit_warnings() {
    let input = include_str!("fixtures/engine/unknown_instructions.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    // EXPOSE and HEALTHCHECK are both unknown → 2 UnsupportedInstruction warnings.
    let unsupported_count = state
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::UnsupportedInstruction { .. }))
        .count();
    assert_eq!(
        unsupported_count, 2,
        "expected 2 UnsupportedInstruction warnings (EXPOSE + HEALTHCHECK)"
    );
}

// ── test: ENV accumulation ────────────────────────────────────────────────────

#[test]
fn test_env_accumulation() {
    let input = include_str!("fixtures/engine/env_accumulation.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    assert_eq!(state.env.get("FIRST"), Some(&"one".to_string()));
    assert_eq!(state.env.get("SECOND"), Some(&"two".to_string()));
    assert_eq!(state.env.get("THIRD"), Some(&"three".to_string()));
}
