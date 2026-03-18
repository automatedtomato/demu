//! Integration fixtures test suite for v0.1.
//!
//! Each test exercises the full `parse_dockerfile() → engine::run() → PreviewState`
//! pipeline using Dockerfiles under `tests/fixtures/integration/`. Assertions
//! document the engine's actual (not aspirational) behaviour so the test suite
//! acts as a living specification.
//!
//! The shared build context directory is `tests/fixtures/integration/context/`,
//! which currently contains `app.conf` and `README.md`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use demu::engine::run;
use demu::model::{fs::FsNode, provenance::ProvenanceSource, warning::Warning};
use demu::parser::parse_dockerfile;
use std::path::Path;

/// Absolute path to the shared build context directory used by all tests.
///
/// Using `CARGO_MANIFEST_DIR` ensures the path resolves correctly regardless
/// of the working directory from which `cargo test` is invoked (e.g. in CI
/// the cwd may differ from the workspace root).
const CONTEXT_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/integration/context"
);

// ── test 1: basic COPY ────────────────────────────────────────────────────────
//
// Fixture: FROM ubuntu:22.04 / WORKDIR /app / COPY app.conf /app/app.conf
//
// Expected engine behaviour (what the engine actually does, verified below):
// - cwd becomes /app after WORKDIR
// - /app/app.conf exists with the real content from the context directory
// - FROM, WORKDIR, and COPY each produce one history entry and one layer (3 total)

#[test]
fn test_basic_copy() {
    let input = include_str!("fixtures/integration/basic_copy.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    // WORKDIR /app must set cwd.
    assert_eq!(state.cwd, std::path::PathBuf::from("/app"));

    // /app/app.conf must exist in the virtual filesystem.
    let node = state
        .fs
        .get(std::path::Path::new("/app/app.conf"))
        .expect("/app/app.conf must exist after COPY app.conf /app/app.conf");

    // File content must match the context file.
    match node {
        FsNode::File(f) => {
            assert!(
                !f.content.is_empty(),
                "/app/app.conf must not be an empty placeholder"
            );
            let content = std::str::from_utf8(&f.content).expect("valid utf-8");
            assert!(
                content.contains("server.port"),
                "app.conf content must contain 'server.port', got: {content:?}"
            );
        }
        _ => panic!("expected FsNode::File at /app/app.conf"),
    }

    // FROM + WORKDIR + COPY = 3 instructions → 3 history entries, 3 layers.
    assert_eq!(
        state.history.len(),
        3,
        "expected 3 history entries (FROM + WORKDIR + COPY)"
    );
    assert_eq!(
        state.layers.len(),
        3,
        "expected 3 layer summaries (FROM + WORKDIR + COPY)"
    );

    // The COPY layer (index 2) must record /app/app.conf in files_changed.
    // This verifies that LayerSummary.files_changed is populated, not just the
    // layer count.
    let copy_layer = &state.layers[2];
    assert_eq!(
        copy_layer.instruction_type, "COPY",
        "layer[2] must be the COPY layer"
    );
    assert!(
        copy_layer
            .files_changed
            .contains(&std::path::PathBuf::from("/app/app.conf")),
        "COPY layer must list /app/app.conf in files_changed, got: {:?}",
        copy_layer.files_changed
    );

    // The copied node must carry CopyFromHost provenance — this is the engine
    // contract that powers the :explain command. Checking it here confirms the
    // full parse → engine → provenance pipeline, not just file existence.
    match node.provenance().created_by {
        ProvenanceSource::CopyFromHost { .. } => {}
        ref other => panic!("expected CopyFromHost provenance on /app/app.conf, got: {other:?}"),
    }

    // The FROM instruction always emits EmptyBaseImage for unstubbed images.
    // This is the primary signal to the user that the filesystem starts empty.
    assert!(
        state.warnings.iter().any(|w| matches!(
            w,
            Warning::EmptyBaseImage { image }
            if image == "ubuntu:22.04"
        )),
        "expected EmptyBaseImage warning for 'ubuntu:22.04', got: {:?}",
        state.warnings
    );
}

// ── test 2: ENV vars ──────────────────────────────────────────────────────────
//
// Fixture: FROM alpine:3.18 / ENV NODE_ENV=production / ENV PORT=3000
//          / ENV APP_NAME my-app
//
// ENV APP_NAME uses the space-separated form — the parser produces key="APP_NAME"
// and value="my-app". All three variables must appear in state.env.

#[test]
fn test_env_vars() {
    let input = include_str!("fixtures/integration/env_vars.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    // All three ENV vars must be present with the correct values.
    assert_eq!(
        state.env.get("NODE_ENV"),
        Some(&"production".to_string()),
        "NODE_ENV must be 'production'"
    );
    assert_eq!(
        state.env.get("PORT"),
        Some(&"3000".to_string()),
        "PORT must be '3000'"
    );
    assert_eq!(
        state.env.get("APP_NAME"),
        Some(&"my-app".to_string()),
        "APP_NAME must be 'my-app' (space-separated ENV form)"
    );

    // FROM + 3 × ENV = 4 instructions.
    assert_eq!(
        state.history.len(),
        4,
        "expected 4 history entries (FROM + 3 ENV)"
    );

    // Each ENV instruction must record its key=value pair in the layer summary.
    // Layer[0]=FROM, [1]=ENV NODE_ENV, [2]=ENV PORT, [3]=ENV APP_NAME.
    let env_layer_1 = &state.layers[1];
    assert_eq!(env_layer_1.instruction_type, "ENV");
    assert!(
        env_layer_1
            .env_changed
            .iter()
            .any(|(k, v)| k == "NODE_ENV" && v == "production"),
        "ENV layer[1] must record NODE_ENV=production in env_changed, got: {:?}",
        env_layer_1.env_changed
    );

    // The space-form ENV (APP_NAME my-app) must also populate env_changed.
    let env_layer_3 = &state.layers[3];
    assert_eq!(env_layer_3.instruction_type, "ENV");
    assert!(
        env_layer_3
            .env_changed
            .iter()
            .any(|(k, v)| k == "APP_NAME" && v == "my-app"),
        "ENV layer[3] must record APP_NAME=my-app in env_changed, got: {:?}",
        env_layer_3.env_changed
    );
}

// ── test 3: RUN history ───────────────────────────────────────────────────────
//
// Fixture: FROM debian:bullseye / RUN apt-get update
//          / RUN apt-get install -y curl wget / RUN echo "setup complete"
//
// The v0.1 engine stubs all RUN commands: each one emits an UnmodeledRunCommand
// warning and records a history entry, but does not mutate the filesystem or env.
// There are 3 RUN instructions so there must be exactly 3 UnmodeledRunCommand warnings.

#[test]
fn test_run_history() {
    let input = include_str!("fixtures/integration/run_history.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    // Every RUN must appear as a history entry.
    // FROM + 3 × RUN = 4 total entries.
    assert_eq!(
        state.history.len(),
        4,
        "expected 4 history entries (FROM + 3 RUN)"
    );

    // Each RUN produces exactly one UnmodeledRunCommand warning.
    let unmodeled_count = state
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::UnmodeledRunCommand { .. }))
        .count();
    assert_eq!(
        unmodeled_count, 3,
        "expected 3 UnmodeledRunCommand warnings (one per RUN)"
    );

    // Verify the specific command text is preserved in warnings.
    let has_apt_update = state.warnings.iter().any(|w| {
        matches!(w, Warning::UnmodeledRunCommand { command } if command.contains("apt-get update"))
    });
    assert!(
        has_apt_update,
        "expected UnmodeledRunCommand for 'apt-get update'"
    );

    // All three RUN command texts must be preserved in history entries.
    // FROM is at index 0; RUN entries follow at 1, 2, 3.
    assert!(
        state.history[1].instruction.contains("apt-get update"),
        "history[1] must contain 'apt-get update', got: {:?}",
        state.history[1].instruction
    );
    assert!(
        state.history[2].instruction.contains("apt-get install"),
        "history[2] must contain 'apt-get install', got: {:?}",
        state.history[2].instruction
    );
    assert!(
        state.history[3].instruction.contains("echo"),
        "history[3] must contain 'echo', got: {:?}",
        state.history[3].instruction
    );

    // The filesystem must be empty — the FROM instruction used in this fixture
    // ("FROM debian:bullseye") has no stub, so the engine starts with an empty
    // virtual filesystem, and the RUN stubs must not insert any files.
    //
    // This assertion documents the current engine contract: RUN stubs do not
    // mutate the filesystem. The provenance check below isolates this claim.
    assert_eq!(
        state.fs.iter().count(),
        0,
        "expected empty filesystem: FROM stub + RUN stubs must not insert any files"
    );

    // Belt-and-suspenders: confirm that no node carries RunCommand provenance.
    // Even if a future FROM implementation adds stub files, this assertion will
    // still catch any RUN stub that incorrectly writes to the filesystem.
    let run_files: Vec<_> = state
        .fs
        .iter()
        .filter(|(_path, node)| {
            matches!(
                node.provenance().created_by,
                ProvenanceSource::RunCommand { .. }
            )
        })
        .collect();
    assert!(
        run_files.is_empty(),
        "no filesystem node must have RunCommand provenance, got: {:?}",
        run_files.iter().map(|(p, _)| p).collect::<Vec<_>>()
    );
}

// ── test 4: missing COPY source ───────────────────────────────────────────────
//
// Fixture: FROM scratch / COPY does_not_exist.txt /app/missing.txt
//
// The engine treats a missing COPY source as a recoverable warning rather than
// a hard error. It emits MissingCopySource and inserts an empty placeholder file
// at the destination so the path is visible in the virtual filesystem.

#[test]
fn test_missing_copy_src() {
    let input = include_str!("fixtures/integration/missing_copy_src.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    // A MissingCopySource warning must have been emitted with the correct path.
    assert!(
        state.warnings.iter().any(|w| matches!(
            w,
            Warning::MissingCopySource { path }
            if path.ends_with("does_not_exist.txt")
        )),
        "expected MissingCopySource warning for 'does_not_exist.txt', got: {:?}",
        state.warnings
    );

    // An empty placeholder file must be inserted at the destination path.
    let node = state
        .fs
        .get(std::path::Path::new("/app/missing.txt"))
        .expect("/app/missing.txt placeholder must exist even when source is absent");

    match node {
        FsNode::File(f) => {
            assert!(
                f.content.is_empty(),
                "placeholder file content must be empty (no real source was read)"
            );
        }
        _ => panic!("expected FsNode::File placeholder at /app/missing.txt"),
    }

    // FROM scratch + COPY = 2 instructions → 2 layers.
    assert_eq!(
        state.layers.len(),
        2,
        "expected 2 layers (FROM + COPY), got: {}",
        state.layers.len()
    );
}

// ── test 5: multi-instruction ─────────────────────────────────────────────────
//
// Fixture: FROM node:20-slim / WORKDIR /srv / ENV NODE_ENV=production
//          / COPY app.conf /srv/app.conf / RUN npm install
//          / COPY README.md /srv/README.md / ENV PORT=8080
//
// This test verifies that all state dimensions are mutated correctly when
// multiple instruction types are interleaved in a single Dockerfile.

#[test]
fn test_multi_instruction() {
    let input = include_str!("fixtures/integration/multi_instruction.dockerfile");
    let instructions = parse_dockerfile(input).expect("parse");
    let state = run(instructions, Path::new(CONTEXT_DIR)).expect("run");

    // WORKDIR /srv must set cwd.
    assert_eq!(
        state.cwd,
        std::path::PathBuf::from("/srv"),
        "cwd must be /srv after WORKDIR /srv"
    );

    // Both ENV vars must be present.
    assert_eq!(
        state.env.get("NODE_ENV"),
        Some(&"production".to_string()),
        "NODE_ENV must be 'production'"
    );
    assert_eq!(
        state.env.get("PORT"),
        Some(&"8080".to_string()),
        "PORT must be '8080'"
    );

    // Both COPYed files must exist in the virtual filesystem with real content.
    let app_conf_node = state
        .fs
        .get(std::path::Path::new("/srv/app.conf"))
        .expect("/srv/app.conf must exist after COPY app.conf /srv/app.conf");
    match app_conf_node {
        FsNode::File(f) => {
            let content = std::str::from_utf8(&f.content).expect("valid utf-8");
            assert!(
                content.contains("server.port"),
                "/srv/app.conf must contain 'server.port', got: {content:?}"
            );
        }
        _ => panic!("expected FsNode::File at /srv/app.conf"),
    }

    assert!(
        state.fs.contains(std::path::Path::new("/srv/README.md")),
        "/srv/README.md must exist after COPY README.md /srv/README.md"
    );

    // FROM + WORKDIR + ENV + COPY + RUN + COPY + ENV = 7 instructions.
    assert_eq!(
        state.history.len(),
        7,
        "expected 7 history entries (FROM + WORKDIR + ENV + COPY + RUN + COPY + ENV)"
    );
    assert_eq!(
        state.layers.len(),
        7,
        "expected 7 layer summaries (one per instruction)"
    );

    // The RUN must have emitted one UnmodeledRunCommand warning.
    assert!(
        state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. })),
        "expected UnmodeledRunCommand warning from 'RUN npm install'"
    );

    // Layer types must appear in the correct order.
    let layer_types: Vec<&str> = state
        .layers
        .iter()
        .map(|l| l.instruction_type.as_str())
        .collect();
    assert_eq!(
        layer_types,
        vec!["FROM", "WORKDIR", "ENV", "COPY", "RUN", "COPY", "ENV"],
        "layers must appear in the order the instructions were processed"
    );
}
