//! End-to-end integration tests: parse fixture Dockerfiles, run through engine,
//! verify PreviewState.
//!
//! Each test loads a fixture Dockerfile from `tests/fixtures/engine/`, parses it
//! with `parse_dockerfile`, then runs it through `engine::run` against the shared
//! build context directory at `tests/fixtures/engine/context/`. Assertions cover
//! filesystem content, environment variables, warnings, and history/layer counts.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use demu::engine::run;
use demu::model::{fs::FsNode, provenance::ProvenanceSource, warning::Warning};
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
    let state = run(instructions, Path::new(CONTEXT_DIR))
        .expect("run")
        .state;

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
    let state = run(instructions, Path::new(CONTEXT_DIR))
        .expect("run")
        .state;

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
    let state = run(instructions, Path::new(CONTEXT_DIR))
        .expect("run")
        .state;

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
    let state = run(instructions, Path::new(CONTEXT_DIR))
        .expect("run")
        .state;

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
    let state = run(instructions, Path::new(CONTEXT_DIR))
        .expect("run")
        .state;

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
    let state = run(instructions, Path::new(CONTEXT_DIR))
        .expect("run")
        .state;

    assert_eq!(state.env.get("FIRST"), Some(&"one".to_string()));
    assert_eq!(state.env.get("SECOND"), Some(&"two".to_string()));
    assert_eq!(state.env.get("THIRD"), Some(&"three".to_string()));
}

// ── test: COPY --from=<stage> copies file between stages ─────────────────────

#[test]
fn test_copy_from_stage_copies_file_between_stages() {
    // Fixture: builder stage copies hello.txt to /build/hello.txt,
    // final stage copies it via `COPY --from=builder /build/hello.txt /app/hello.txt`.
    let input = include_str!("fixtures/engine/copy_from_stage.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let output = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    // /app/hello.txt must exist in the final stage.
    let node = output
        .state
        .fs
        .get(Path::new("/app/hello.txt"))
        .expect("/app/hello.txt must exist in final stage");

    match node {
        FsNode::File(f) => {
            // Content was copied from the builder stage which read it from host.
            assert_eq!(
                f.content, b"hello from context",
                "content must propagate through stage copy"
            );
            // Provenance must record a CopyFromStage origin.
            match &f.provenance.created_by {
                ProvenanceSource::CopyFromStage { stage } => {
                    assert_eq!(stage, "builder", "provenance must name 'builder' stage");
                }
                other => panic!("expected CopyFromStage provenance, got: {other:?}"),
            }
        }
        _ => panic!("expected File node at /app/hello.txt"),
    }
}

// ── test: COPY --from=<nonexistent> emits MissingCopyStage warning ────────────

#[test]
fn test_copy_from_stage_missing_stage_emits_warning() {
    // Inline Dockerfile: single stage tries to copy from a stage "nonexistent"
    // that does not exist in the registry.
    let input = "FROM scratch\nCOPY --from=nonexistent /app /out\n";
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR))
        .expect("run")
        .state;

    let has_warning = state
        .warnings
        .iter()
        .any(|w| matches!(w, Warning::MissingCopyStage { stage, .. } if stage == "nonexistent"));
    assert!(
        has_warning,
        "expected MissingCopyStage warning for 'nonexistent'; warnings: {:?}",
        state.warnings
    );
}

// ── test: COPY --from=0 (numeric index) copies file ──────────────────────────

#[test]
fn test_copy_from_stage_numeric_index_works() {
    // Inline two-stage Dockerfile: stage 0 creates /build/app.txt,
    // stage 1 copies it via `COPY --from=0`.
    let input = concat!(
        "FROM ubuntu:22.04 AS builder\n",
        "COPY hello.txt /build/hello.txt\n",
        "FROM alpine:3.18\n",
        "COPY --from=0 /build/hello.txt /app/hello.txt\n",
    );
    let instructions = parse_dockerfile(input).expect("parse");
    let output = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    // /app/hello.txt must exist — copied by numeric index "0".
    let node = output
        .state
        .fs
        .get(Path::new("/app/hello.txt"))
        .expect("/app/hello.txt must exist after numeric --from=0 copy");

    match node {
        FsNode::File(f) => {
            assert_eq!(
                f.content, b"hello from context",
                "numeric index copy must preserve content"
            );
        }
        _ => panic!("expected File node"),
    }
}
