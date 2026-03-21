// Integration tests for the Compose engine (issue #50).
//
// These tests use fixture files under `tests/fixtures/compose/engine/` to
// exercise the full parse-and-run pipeline end-to-end, asserting on the
// resulting `PreviewState` fields.

use std::path::{Path, PathBuf};

use demu::{engine::run_compose, model::warning::Warning, parser::compose::parse_compose};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Path to the fixture directory for compose engine tests.
fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compose/engine")
}

/// Parse a compose file fixture by name and return the `ComposeFile`.
fn parse_fixture(filename: &str) -> demu::model::compose::ComposeFile {
    let path = fixture_dir().join(filename);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read fixture '{}': {e}", path.display()));
    parse_compose(&content)
        .unwrap_or_else(|e| panic!("failed to parse fixture '{}': {e}", path.display()))
}

// ── test 1: build service produces Dockerfile filesystem ─────────────────────

#[test]
fn build_service_produces_dockerfile_filesystem() {
    // compose_build.yaml points at the Dockerfile in the same directory.
    // The Dockerfile sets WORKDIR /app, so /app must be in the virtual FS.
    let compose = parse_fixture("compose_build.yaml");
    let dir = fixture_dir();

    let output = run_compose(&compose, "api", &dir).expect("compose engine must succeed");

    // /app should exist from the Dockerfile's WORKDIR instruction.
    let app_path = PathBuf::from("/app");
    assert!(
        output.state.fs.contains(&app_path),
        "virtual fs must contain /app from Dockerfile WORKDIR"
    );

    // DF_VAR is set in the Dockerfile; compose_build.yaml doesn't override it.
    assert_eq!(
        output.state.env.get("DF_VAR").map(String::as_str),
        Some("from_dockerfile"),
        "DF_VAR must be set from Dockerfile ENV"
    );
}

// ── test 2: compose environment wins over dockerfile ENV ─────────────────────

#[test]
fn compose_environment_wins_over_dockerfile_env() {
    // compose_build.yaml: environment: [SHARED=from_compose, COMPOSE_VAR=from_compose]
    // Dockerfile:         ENV SHARED=from_dockerfile
    let compose = parse_fixture("compose_build.yaml");
    let dir = fixture_dir();

    let output = run_compose(&compose, "api", &dir).expect("compose engine must succeed");

    // Compose environment must override Dockerfile ENV for SHARED.
    assert_eq!(
        output.state.env.get("SHARED").map(String::as_str),
        Some("from_compose"),
        "SHARED must be from Compose environment, not Dockerfile ENV"
    );

    // COMPOSE_VAR is Compose-only (not in Dockerfile).
    assert_eq!(
        output.state.env.get("COMPOSE_VAR").map(String::as_str),
        Some("from_compose"),
        "COMPOSE_VAR must be present from Compose environment"
    );
}

// ── test 3: working_dir overrides Dockerfile WORKDIR ─────────────────────────

#[test]
fn working_dir_overrides_dockerfile_workdir() {
    // compose_build.yaml sets working_dir: /override
    let compose = parse_fixture("compose_build.yaml");
    let dir = fixture_dir();

    let output = run_compose(&compose, "api", &dir).expect("compose engine must succeed");

    assert_eq!(
        output.state.cwd,
        PathBuf::from("/override"),
        "cwd must be /override from compose working_dir"
    );

    // /override must also exist in the virtual filesystem.
    assert!(
        output.state.fs.contains(&PathBuf::from("/override")),
        "virtual fs must contain /override"
    );
}

// ── test 4: image-only service has empty filesystem with warning ──────────────

#[test]
fn image_only_service_empty_fs_with_warning() {
    let compose = parse_fixture("compose_image_only.yaml");
    let dir = fixture_dir();

    let output = run_compose(&compose, "db", &dir).expect("compose engine must succeed");

    // Filesystem must be empty (no Dockerfile ran).
    assert_eq!(
        output.state.fs.iter().count(),
        0,
        "image-only service must have empty fs"
    );

    // ImageOnlyService warning must be present.
    let has_warning =
        output.state.warnings.iter().any(
            |w| matches!(w, Warning::ImageOnlyService { image } if image.contains("postgres")),
        );
    assert!(
        has_warning,
        "must have ImageOnlyService warning for postgres"
    );
}

// ── test 5: image-only service gets compose environment applied ───────────────

#[test]
fn image_only_service_gets_compose_environment() {
    // compose_image_only.yaml: environment: [DB_VAR=hello]
    let compose = parse_fixture("compose_image_only.yaml");
    let dir = fixture_dir();

    let output = run_compose(&compose, "db", &dir).expect("compose engine must succeed");

    assert_eq!(
        output.state.env.get("DB_VAR").map(String::as_str),
        Some("hello"),
        "DB_VAR from compose environment must be applied even for image-only service"
    );
}

// ── test 6: env_file is loaded and merged ─────────────────────────────────────

#[test]
fn env_file_loaded_and_merged() {
    // compose_env_file.yaml references .env.test, which sets:
    //   ENV_FILE_VAR=from_env_file
    //   SHARED=from_env_file
    // The compose file also sets environment: [SHARED=from_compose]
    // So SHARED should be "from_compose" (environment wins over env_file).
    let compose = parse_fixture("compose_env_file.yaml");
    let dir = fixture_dir();

    let output = run_compose(&compose, "api", &dir).expect("compose engine must succeed");

    assert_eq!(
        output.state.env.get("ENV_FILE_VAR").map(String::as_str),
        Some("from_env_file"),
        "ENV_FILE_VAR must be loaded from env_file"
    );

    // Compose environment wins over env_file for SHARED.
    assert_eq!(
        output.state.env.get("SHARED").map(String::as_str),
        Some("from_compose"),
        "SHARED: environment entry must win over env_file entry"
    );
}

// ── test 7: missing env_file emits warning without crashing ───────────────────

#[test]
fn missing_env_file_emits_warning_without_crash() {
    let compose = parse_fixture("compose_missing_env_file.yaml");
    let dir = fixture_dir();

    // Must not return an error — missing env_file is non-fatal.
    let output =
        run_compose(&compose, "api", &dir).expect("missing env_file must not be a fatal error");

    let has_warning = output
        .state
        .warnings
        .iter()
        .any(|w| matches!(w, Warning::EnvFileNotFound { .. }));
    assert!(has_warning, "must have EnvFileNotFound warning");
}

// ── test 8: KeyOnly env emits UnresolvedEnvKey warning ────────────────────────

#[test]
fn key_only_env_emits_unresolved_warning() {
    // compose_key_only_env.yaml: environment: [HOST_VAR, DEFINED_VAR=has_value]
    let compose = parse_fixture("compose_key_only_env.yaml");
    let dir = fixture_dir();

    let output = run_compose(&compose, "api", &dir).expect("should succeed");

    let has_warning = output
        .state
        .warnings
        .iter()
        .any(|w| matches!(w, Warning::UnresolvedEnvKey { key } if key == "HOST_VAR"));
    assert!(
        has_warning,
        "must have UnresolvedEnvKey warning for HOST_VAR"
    );

    // HOST_VAR must NOT be in env.
    assert!(
        !output.state.env.contains_key("HOST_VAR"),
        "HOST_VAR must not be in state.env"
    );

    // DEFINED_VAR must be in env.
    assert_eq!(
        output.state.env.get("DEFINED_VAR").map(String::as_str),
        Some("has_value"),
        "DEFINED_VAR must be present with correct value"
    );
}

// ── test 9: output fields are correct ─────────────────────────────────────────

#[test]
fn output_selected_service_and_compose_file_are_correct() {
    let compose = parse_fixture("compose_build.yaml");
    let dir = fixture_dir();

    let output = run_compose(&compose, "api", &dir).expect("should succeed");

    assert_eq!(output.selected_service, "api");
    assert!(output.compose_file.services.contains_key("api"));
}
