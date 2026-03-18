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
        !stderr.is_empty(),
        "stderr must contain an error message, got empty"
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
        stderr.contains("warning:") || stderr.contains("ubuntu"),
        "stderr must contain a warning about the unmodeled base image, got: {stderr}"
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
        !stderr.is_empty(),
        "directory-as-file error must produce a stderr message"
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

// ── test 10: stderr error message includes "demu:" prefix ────────────────────

#[test]
fn fatal_error_message_includes_demu_prefix() {
    // When `run_cli()` returns an `Err`, `main()` formats it as "demu: <msg>"
    // before printing to stderr. This makes the binary behave consistently with
    // other Unix tools (git, cargo, etc.) where the program name prefixes errors.
    let output = demu()
        .args(["-f", "/nonexistent/Dockerfile"])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run demu");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("demu:"),
        "fatal error must begin with 'demu:', got: {stderr}"
    );
}
