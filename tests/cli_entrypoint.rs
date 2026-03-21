// Binary-level integration tests for the `demu` CLI entrypoint.
//
// These tests invoke the compiled `demu` binary via `std::process::Command`
// and assert on exit codes and output. They cover the full pipeline:
// argument validation → file I/O → parse → engine → warnings → REPL lifecycle.
//
// `env!("CARGO_BIN_EXE_demu")` resolves to the path of the compiled binary at
// test time, so no hard-coded paths are needed.

use std::io::Write;
use std::process::{Command, Stdio};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Return a `Command` targeting the compiled `demu` binary.
fn demu() -> Command {
    Command::new(env!("CARGO_BIN_EXE_demu"))
}

/// Create a temporary Dockerfile containing `content` and return its path.
///
/// The returned `tempfile::NamedTempFile` keeps the file alive for the
/// duration of the test — do not drop it before `Command::status()` returns.
fn temp_dockerfile(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("tempfile");
    f.write_all(content.as_bytes()).expect("write dockerfile");
    f
}

/// Create a temporary Compose YAML file containing `content` and return its path.
///
/// The returned `tempfile::NamedTempFile` keeps the file alive for the
/// duration of the test — do not drop it before `Command::status()` returns.
fn temp_compose_file(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("tempfile");
    f.write_all(content.as_bytes()).expect("write compose file");
    f
}

/// Minimal valid compose YAML with an "api" service.
const COMPOSE_WITH_API: &str = "services:\n  api:\n    image: myapp:latest\n";

/// Compose YAML with two services for service-list testing.
const COMPOSE_WITH_API_AND_DB: &str =
    "services:\n  api:\n    image: myapp:latest\n  db:\n    image: postgres:15\n";

// ── test 1: missing --file flag exits non-zero ────────────────────────────────

#[test]
fn no_args_exits_nonzero() {
    // `demu` with no arguments must exit with a non-zero status code because
    // the `-f` / `--file` flag is required.
    let status = demu()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("failed to run demu");

    assert!(
        !status.success(),
        "expected non-zero exit when -f is missing"
    );
}

// ── test 2: non-existent Dockerfile path exits 1 ─────────────────────────────

#[test]
fn nonexistent_file_exits_one_with_error_message() {
    // When the path passed to `-f` does not exist, `demu` must exit 1 and
    // print a diagnostic to stderr so the user knows what went wrong.
    let output = demu()
        .args(["-f", "/this/path/does/not/exist/Dockerfile"])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert_eq!(
        output.status.code(),
        Some(1),
        "non-existent file must exit with code 1"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("demu:"),
        "stderr must begin with 'demu:' prefix, got: {stderr}"
    );
    assert!(
        stderr.contains("not found") || stderr.contains("No such"),
        "stderr must mention that the file was not found, got: {stderr}"
    );
}

// ── test 3: --version exits 0 and prints version ─────────────────────────────

#[test]
fn version_flag_exits_zero_and_prints_version() {
    // `--version` is provided by clap. It must exit 0 and emit the crate
    // version string to stdout so callers can check compatibility.
    let output = demu()
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu --version");

    assert!(
        output.status.success(),
        "--version must exit 0, got: {:?}",
        output.status
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Clap prints "<name> <version>" — the version from Cargo.toml must appear.
    assert!(
        stdout.contains("0.1.0"),
        "--version must contain the crate version, got: {stdout}"
    );
}

// ── test 4: --help exits 0 and mentions -f ────────────────────────────────────

#[test]
fn help_flag_exits_zero_and_documents_file_flag() {
    // `--help` must exit 0 and surface the `-f` / `--file` flag in its output
    // so users know how to specify a Dockerfile.
    let output = demu()
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu --help");

    assert!(
        output.status.success(),
        "--help must exit 0, got: {:?}",
        output.status
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("-f") || stdout.contains("--file"),
        "--help must document the -f/--file flag, got: {stdout}"
    );
}

// ── test 5: valid Dockerfile with closed stdin exits 0 (REPL hits EOF) ───────

#[test]
fn valid_dockerfile_with_eof_stdin_exits_zero() {
    // When a well-formed Dockerfile is provided and stdin is closed immediately
    // (simulating Ctrl-D), rustyline returns `ReadlineError::Eof` and the REPL
    // exits cleanly with code 0.
    //
    // Uses `FROM scratch` — the simplest valid instruction — so the engine
    // produces at most an EmptyBaseImage warning but does not error out.
    let dockerfile = temp_dockerfile("FROM scratch\n");

    let output = demu()
        .args(["-f", dockerfile.path().to_str().expect("path to str")])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert!(
        output.status.success(),
        "valid Dockerfile + EOF stdin must exit 0, got: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── test 6: warnings are emitted to stderr ────────────────────────────────────

#[test]
fn warnings_are_printed_to_stderr() {
    // The engine emits an `EmptyBaseImage` warning for every FROM instruction
    // (because no image stubs are modeled yet). The CLI must forward these
    // warnings to stderr so users understand the approximation.
    let dockerfile = temp_dockerfile("FROM ubuntu:22.04\n");

    let output = demu()
        .args(["-f", dockerfile.path().to_str().expect("path to str")])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning:"),
        "stderr must contain a 'warning:' prefixed message about the unmodeled base image, got: {stderr}"
    );
}

// ── test 7: path that exists but is a directory exits 1 ──────────────────────

#[test]
fn directory_path_exits_one() {
    // If the user passes a directory instead of a file, `demu` must exit 1 with
    // a meaningful error rather than panicking or silently succeeding.
    let dir = tempfile::tempdir().expect("tempdir");

    let output = demu()
        .args(["-f", dir.path().to_str().expect("path to str")])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert_eq!(
        output.status.code(),
        Some(1),
        "directory path must exit 1, got: {:?}",
        output.status
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("demu:"),
        "directory error must begin with 'demu:' prefix, got: {stderr}"
    );
    assert!(
        stderr.contains("not a regular file") || stderr.contains("is not"),
        "directory error must mention the path is not a regular file, got: {stderr}"
    );
}

// ── test 8: parse error exits 1 ───────────────────────────────────────────────

#[test]
fn malformed_dockerfile_exits_one() {
    // A Dockerfile with a malformed instruction (e.g., bare `FROM` with no
    // image) must cause `demu` to exit 1 — the parse error propagates up
    // through `run_cli()` and is printed to stderr by `main()`.
    let dockerfile = temp_dockerfile("FROM\n");

    let output = demu()
        .args(["-f", dockerfile.path().to_str().expect("path to str")])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert_eq!(
        output.status.code(),
        Some(1),
        "malformed Dockerfile must exit 1, got: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("demu:"),
        "parse error must begin with 'demu:' prefix, got: {stderr}"
    );
    assert!(
        stderr.contains("parse"),
        "parse error must mention 'parse', got: {stderr}"
    );
}

// ── test 9: multi-instruction Dockerfile exits 0 with EOF stdin ───────────────

#[test]
fn multi_instruction_dockerfile_with_eof_stdin_exits_zero() {
    // A more realistic Dockerfile (FROM + WORKDIR + ENV) must also produce a
    // clean exit when stdin is closed. This exercises the full parse → engine
    // → REPL pipeline end-to-end.
    let content = "FROM ubuntu:22.04\nWORKDIR /app\nENV APP_ENV=production\n";
    let dockerfile = temp_dockerfile(content);

    let output = demu()
        .args(["-f", dockerfile.path().to_str().expect("path to str")])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert!(
        output.status.success(),
        "multi-instruction Dockerfile + EOF stdin must exit 0, got: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── test 10: --stage with nonexistent name exits 1 with a clear error ────────

#[test]
fn stage_flag_exits_one_with_not_implemented_error() {
    // Requesting a stage name that does not exist must exit 1 with a clear
    // error message that names the missing stage and lists available stages.
    // "FROM scratch" produces only stage "0" (no alias), so "builder" is unknown.
    let dockerfile = temp_dockerfile("FROM scratch\n");

    let output = demu()
        .args([
            "-f",
            dockerfile.path().to_str().expect("path to str"),
            "--stage",
            "builder",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert_eq!(
        output.status.code(),
        Some(1),
        "--stage with unknown stage must exit 1, got: {:?}",
        output.status
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("demu:"),
        "--stage error must begin with 'demu:', got: {stderr}"
    );
    assert!(
        stderr.contains("stage"),
        "--stage error must mention 'stage', got: {stderr}"
    );
    assert!(
        stderr.contains("not found"),
        "--stage error must say 'not found', got: {stderr}"
    );
    // The error must list available stages so the user knows what to pass.
    assert!(
        stderr.contains("available stages"),
        "--stage error must list 'available stages', got: {stderr}"
    );
}

// ── test 10b: --stage <valid-name> selects correct stage ─────────────────────

#[test]
fn valid_stage_name_exits_zero() {
    // A two-stage Dockerfile with a named builder stage. Passing --stage builder
    // should select the builder stage and exit 0 with EOF stdin.
    let df = temp_dockerfile(
        "FROM ubuntu:22.04 AS builder\nWORKDIR /build\nRUN echo done\nFROM scratch\nCOPY --from=builder /build /out\n",
    );
    let output = demu()
        .args(["-f", df.path().to_str().expect("path"), "--stage", "builder"])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert_eq!(
        output.status.code(),
        Some(0),
        "--stage builder on valid two-stage Dockerfile must exit 0, got: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── Compose mode tests ────────────────────────────────────────────────────────

// ── test 11: --compose without --service exits 1 with clear error ─────────────

#[test]
fn compose_without_service_flag_exits_one() {
    // `--compose` requires `--service`. Missing `--service` must exit 1 with
    // a message that names the missing flag so the user knows how to fix it.
    let compose = temp_compose_file(COMPOSE_WITH_API);

    let output = demu()
        .args(["--compose", "-f", compose.path().to_str().expect("path")])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert_eq!(
        output.status.code(),
        Some(1),
        "--compose without --service must exit 1, got: {:?}",
        output.status
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--service") || stderr.contains("service"),
        "error must mention '--service', got: {stderr}"
    );
    assert!(
        stderr.contains("required") || stderr.contains("compose mode"),
        "error must say 'required' or 'compose mode', got: {stderr}"
    );
}

// ── test 12: --compose with nonexistent service exits 1 and lists services ────

#[test]
fn compose_nonexistent_service_exits_one_with_available_list() {
    // When `--service` names a service that does not exist in the Compose file,
    // `demu` must exit 1 and list the available service names so the user knows
    // what to pass.
    let compose = temp_compose_file(COMPOSE_WITH_API_AND_DB);

    let output = demu()
        .args([
            "--compose",
            "-f",
            compose.path().to_str().expect("path"),
            "--service",
            "nonexistent",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert_eq!(
        output.status.code(),
        Some(1),
        "--compose with nonexistent service must exit 1, got: {:?}",
        output.status
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found"),
        "error must say 'not found', got: {stderr}"
    );
    // Both service names from the compose file must appear so the user can pick.
    assert!(
        stderr.contains("api"),
        "error must list available service 'api', got: {stderr}"
    );
    assert!(
        stderr.contains("db"),
        "error must list available service 'db', got: {stderr}"
    );
}

// ── test 13: --compose with invalid YAML exits 1 with parse error ─────────────

#[test]
fn compose_invalid_yaml_exits_one() {
    // A file that is not valid YAML must cause `demu` to exit 1 with a message
    // that begins with `"demu:"` and references the parse failure.
    let compose = temp_compose_file("{ not: valid: yaml: [\n");

    let output = demu()
        .args([
            "--compose",
            "-f",
            compose.path().to_str().expect("path"),
            "--service",
            "api",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert_eq!(
        output.status.code(),
        Some(1),
        "invalid YAML in compose mode must exit 1, got: {:?}",
        output.status
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("demu:"),
        "error must begin with 'demu:', got: {stderr}"
    );
}

// ── test 14: valid --compose + --service exits 0 with EOF stdin ───────────────

#[test]
fn compose_valid_service_with_eof_stdin_exits_zero() {
    // The happy path: a valid Compose file and a known service name. With stdin
    // closed immediately, the REPL hits EOF and exits cleanly.
    let compose = temp_compose_file(COMPOSE_WITH_API);

    let output = demu()
        .args([
            "--compose",
            "-f",
            compose.path().to_str().expect("path"),
            "--service",
            "api",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert!(
        output.status.success(),
        "--compose with valid service + EOF stdin must exit 0, got: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── test 15: Dockerfile pipeline unchanged in compose mode addition ───────────

#[test]
fn dockerfile_pipeline_unchanged_after_compose_addition() {
    // Regression guard: `demu -f Dockerfile` must still work exactly as before.
    // This ensures the compose branch does not accidentally break the existing flow.
    let dockerfile = temp_dockerfile("FROM scratch\n");

    let output = demu()
        .args(["-f", dockerfile.path().to_str().expect("path")])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert!(
        output.status.success(),
        "existing Dockerfile pipeline must still exit 0, got: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── test 16: --service without --compose emits warning but still exits 0 ──────

#[test]
fn service_without_compose_emits_warning() {
    // Passing --service without --compose should not abort, but should emit
    // a clear warning to stderr so the user is not silently misled.
    let dockerfile = temp_dockerfile("FROM scratch\n");

    let output = demu()
        .args([
            "-f",
            dockerfile.path().to_str().expect("path"),
            "--service",
            "api",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert!(
        output.status.success(),
        "--service without --compose must still exit 0; got: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--service"),
        "--service without --compose must mention '--service' in the warning; got: {stderr}"
    );
    assert!(
        stderr.contains("warning"),
        "--service without --compose must be prefixed with 'warning'; got: {stderr}"
    );
}

// ── test 11 (original): empty Dockerfile (no instructions) exits 0 ───────────

#[test]
fn empty_dockerfile_with_eof_stdin_exits_zero() {
    // A Dockerfile with no instructions (blank or comment-only) is syntactically
    // valid. The engine produces a default PreviewState and the REPL starts
    // normally. With closed stdin it must exit 0.
    let dockerfile = temp_dockerfile("# just a comment\n");

    let output = demu()
        .args(["-f", dockerfile.path().to_str().expect("path to str")])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    assert!(
        output.status.success(),
        "empty/comment Dockerfile + EOF stdin must exit 0, got: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}
