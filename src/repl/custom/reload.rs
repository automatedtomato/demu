// `:reload` REPL command — re-reads the Dockerfile, re-parses, re-runs the
// engine, and replaces `PreviewState`.
//
// Error handling strategy:
// - File read errors   → write message to err_writer, preserve old state, return Ok(())
// - Parse errors       → write message to err_writer, preserve old state, return Ok(())
// - Engine errors      → write message to err_writer, preserve old state, return Ok(())
// - I/O write errors   → return Err(ReplError::Io)
//
// On success, warnings from the new state are emitted to `err_writer` before
// the confirmation message is written to `writer`.

use std::io::Write;

use crate::engine;
use crate::model::state::PreviewState;
use crate::output::sanitize::sanitize_for_terminal;
use crate::parser::{compose::parse_compose, dockerfile::parse_dockerfile};
use crate::repl::config::ReplConfig;
use crate::repl::error::{io_err_mapper, ReplError};

/// Execute the `:reload` command.
///
/// Re-reads the Dockerfile from `config.dockerfile_path`, re-parses it, and
/// re-runs the engine. On success, `state` is replaced with the new state.
/// On any error, `state` is left untouched and the error message is written to
/// `err_writer` — the REPL loop continues normally.
///
/// Warnings from the new simulation are emitted to `err_writer` before the
/// confirmation message is written to `writer`.
pub fn execute(
    state: &mut PreviewState,
    config: &ReplConfig,
    writer: &mut impl Write,
    err_writer: &mut impl Write,
) -> Result<(), ReplError> {
    // Compose mode: re-parse the compose file and re-run the compose engine.
    // The Dockerfile pipeline is the `else` branch below.
    if config.compose_context.is_some() {
        return execute_compose(state, config, writer, err_writer);
    }

    // Step 1: Read the Dockerfile from disk.
    // Sanitize the path before embedding it in terminal output — the path
    // comes from the CLI argument and could theoretically contain control bytes.
    let content = match std::fs::read_to_string(&config.dockerfile_path) {
        Ok(c) => c,
        Err(e) => {
            // Include the OS error reason so the user can distinguish "not found"
            // from "permission denied" or other I/O failures.
            let safe_path = sanitize_for_terminal(&config.dockerfile_path.display().to_string());
            writeln!(
                err_writer,
                "demu: cannot read Dockerfile '{}': {e}",
                safe_path
            )
            .map_err(io_err_mapper(":reload"))?;
            return Ok(());
        }
    };

    // Step 2: Parse the Dockerfile content into instructions.
    let instructions = match parse_dockerfile(&content) {
        Ok(i) => i,
        Err(e) => {
            let safe_msg = sanitize_for_terminal(&e.to_string());
            writeln!(err_writer, "demu: parse error: {safe_msg}")
                .map_err(io_err_mapper(":reload"))?;
            return Ok(());
        }
    };

    // Step 3: Run the engine against the parsed instructions.
    let engine_output = match engine::run(instructions, &config.context_dir) {
        Ok(output) => output,
        Err(e) => {
            let safe_msg = sanitize_for_terminal(&e.to_string());
            writeln!(err_writer, "demu: engine error: {safe_msg}")
                .map_err(io_err_mapper(":reload"))?;
            return Ok(());
        }
    };

    // Step 3b: Re-apply the stage selection from startup, if any.
    // If the selected stage no longer exists after reload (e.g. the user
    // renamed or deleted it in the Dockerfile), emit a warning and fall back
    // to the final stage so the REPL remains usable.
    let new_state = if let Some(ref stage_name) = config.selected_stage {
        match engine_output.stages.get(stage_name) {
            Some(stage_state) => stage_state.clone(),
            None => {
                let safe_name = sanitize_for_terminal(stage_name);
                writeln!(
                    err_writer,
                    "warning: stage '{safe_name}' no longer exists after reload; using final stage"
                )
                .map_err(io_err_mapper(":reload"))?;
                engine_output.state
            }
        }
    } else {
        engine_output.state
    };

    // Step 4a: Emit warnings from the new state to err_writer.
    // Sanitize warning strings — they embed user-supplied Dockerfile data.
    for w in &new_state.warnings {
        writeln!(
            err_writer,
            "warning: {}",
            sanitize_for_terminal(&w.to_string())
        )
        .map_err(io_err_mapper(":reload"))?;
    }

    // Step 4b: Count instructions processed (history entries) before replacing state.
    let n = new_state.history.len();

    // Step 4c: Replace state on success.
    *state = new_state;

    // Step 4d: Confirm success to the user.
    writeln!(writer, "Reloaded. {n} instructions processed.").map_err(io_err_mapper(":reload"))?;

    Ok(())
}

/// Reload for Compose mode.
///
/// Re-reads the compose file, re-parses it, and re-runs the compose engine
/// for the previously selected service. On any error, the old state is
/// preserved and the error is written to `err_writer`.
fn execute_compose(
    state: &mut PreviewState,
    config: &ReplConfig,
    writer: &mut impl Write,
    err_writer: &mut impl Write,
) -> Result<(), ReplError> {
    // The compose context must be present when this function is called.
    // The caller (`execute`) routes here only when `compose_context.is_some()`.
    // Return early rather than panic to maintain the REPL's non-crashing guarantee.
    let ctx = match config.compose_context.as_ref() {
        Some(ctx) => ctx,
        None => return Ok(()),
    };

    // Step 1: Re-read the compose file.
    let content = match std::fs::read_to_string(&config.dockerfile_path) {
        Ok(c) => c,
        Err(e) => {
            let safe_path = sanitize_for_terminal(&config.dockerfile_path.display().to_string());
            writeln!(
                err_writer,
                "demu: cannot read compose file '{}': {e}",
                safe_path
            )
            .map_err(io_err_mapper(":reload"))?;
            return Ok(());
        }
    };

    // Step 2: Re-parse the compose file.
    let compose_file = match parse_compose(&content) {
        Ok(cf) => cf,
        Err(e) => {
            let safe_msg = sanitize_for_terminal(&e.to_string());
            writeln!(err_writer, "demu: compose parse error: {safe_msg}")
                .map_err(io_err_mapper(":reload"))?;
            return Ok(());
        }
    };

    // Step 3: Re-run the compose engine.
    let compose_output =
        match engine::run_compose(&compose_file, &ctx.selected_service, &config.context_dir) {
            Ok(out) => out,
            Err(e) => {
                let safe_msg = sanitize_for_terminal(&e.to_string());
                writeln!(err_writer, "demu: compose engine error: {safe_msg}")
                    .map_err(io_err_mapper(":reload"))?;
                return Ok(());
            }
        };

    // Step 4: Emit warnings, replace state, confirm.
    let new_state = compose_output.state;
    for w in &new_state.warnings {
        writeln!(
            err_writer,
            "warning: {}",
            sanitize_for_terminal(&w.to_string())
        )
        .map_err(io_err_mapper(":reload"))?;
    }

    let n = new_state.history.len();
    *state = new_state;
    writeln!(writer, "Reloaded. {n} instructions processed.").map_err(io_err_mapper(":reload"))?;

    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::compose::ComposeFile;
    use crate::repl::config::{ComposeContext, ReplConfig};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    // Helper: run execute and collect both stdout and stderr into Strings.
    fn run_reload(
        state: &mut PreviewState,
        config: &ReplConfig,
    ) -> (Result<(), ReplError>, String, String) {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let result = execute(state, config, &mut out, &mut err);
        (
            result,
            String::from_utf8(out).expect("utf-8 stdout"),
            String::from_utf8(err).expect("utf-8 stderr"),
        )
    }

    // --- reload_success_replaces_state ---
    //
    // Writing a valid Dockerfile with ENV FOO=bar; after reload the state
    // must contain env["FOO"] == "bar" and history.len() == 2 (FROM + ENV).

    #[test]
    fn reload_success_replaces_state() {
        let mut file = NamedTempFile::new().expect("tempfile");
        write!(file, "FROM scratch\nENV FOO=bar\n").expect("write");

        let config = ReplConfig::new(file.path().to_path_buf());
        let mut state = PreviewState::default();

        let (result, out, _err) = run_reload(&mut state, &config);
        assert!(result.is_ok(), "execute must return Ok; got: {result:?}");
        assert_eq!(
            state.env.get("FOO").map(String::as_str),
            Some("bar"),
            "state.env must contain FOO=bar after reload"
        );
        assert!(
            out.contains("Reloaded."),
            "stdout must contain 'Reloaded.'; got: {out}"
        );
        // FROM + ENV = 2 history entries.
        assert_eq!(
            state.history.len(),
            2,
            "history must have 2 entries; got: {}",
            state.history.len()
        );
    }

    // --- reload_success_shows_instruction_count ---
    //
    // FROM scratch + WORKDIR /app + ENV X=1 = 3 instructions → "3 instructions processed".

    #[test]
    fn reload_success_shows_instruction_count() {
        let mut file = NamedTempFile::new().expect("tempfile");
        write!(file, "FROM scratch\nWORKDIR /app\nENV X=1\n").expect("write");

        let config = ReplConfig::new(file.path().to_path_buf());
        let mut state = PreviewState::default();

        let (result, out, _err) = run_reload(&mut state, &config);
        assert!(result.is_ok());
        assert!(
            out.contains("3 instructions processed"),
            "stdout must contain '3 instructions processed'; got: {out}"
        );
    }

    // --- reload_file_not_found_keeps_old_state ---
    //
    // Config points to a non-existent path. Old state (cwd=/original) must
    // be preserved. stderr must contain "Dockerfile not found".

    #[test]
    fn reload_file_not_found_keeps_old_state() {
        // Drop a NamedTempFile immediately so the path is guaranteed not to exist.
        let gone_path = {
            let tmp = NamedTempFile::new().expect("tempfile");
            tmp.path().to_path_buf()
            // tmp is dropped here — file is deleted
        };
        let config = ReplConfig::new(gone_path);

        let mut state = PreviewState::default();
        state.cwd = PathBuf::from("/original");

        let (result, _out, err) = run_reload(&mut state, &config);
        assert!(
            result.is_ok(),
            "execute must return Ok even on file-not-found; got: {result:?}"
        );
        assert_eq!(
            state.cwd,
            PathBuf::from("/original"),
            "state.cwd must remain /original; got: {}",
            state.cwd.display()
        );
        assert!(
            err.contains("cannot read Dockerfile"),
            "stderr must mention read failure; got: {err}"
        );
    }

    // --- reload_parse_error_keeps_old_state ---
    //
    // File exists but contains invalid Dockerfile syntax. Old state is preserved,
    // stderr contains "parse error".
    //
    // "FROM" with no image triggers ParseError::InvalidInstruction because the
    // parser expects at least one token after the FROM keyword. Unknown keywords
    // are turned into Instruction::Unknown (not errors), so we use a known keyword
    // with missing required arguments to guarantee a real parse error.

    #[test]
    fn reload_parse_error_keeps_old_state() {
        let mut file = NamedTempFile::new().expect("tempfile");
        // "FROM" with no image name — triggers ParseError::InvalidInstruction.
        write!(file, "FROM\n").expect("write");

        let config = ReplConfig::new(file.path().to_path_buf());
        let mut state = PreviewState::default();
        state.cwd = PathBuf::from("/original");

        let (result, _out, err) = run_reload(&mut state, &config);
        assert!(
            result.is_ok(),
            "execute must return Ok even on parse error; got: {result:?}"
        );
        assert_eq!(
            state.cwd,
            PathBuf::from("/original"),
            "state must be preserved on parse error"
        );
        assert!(
            err.contains("parse error"),
            "stderr must contain 'parse error'; got: {err}"
        );
    }

    // --- reload_after_file_change_picks_up_new_content ---
    //
    // Reload picks up content changes: first load sees ENV A=first; after
    // overwriting the file, second reload sees ENV B=second and A is gone.

    #[test]
    fn reload_after_file_change_picks_up_new_content() {
        let mut file = NamedTempFile::new().expect("tempfile");
        // v1: ENV A=first
        write!(file, "FROM scratch\nENV A=first\n").expect("write v1");

        let df_path = file.path().to_path_buf();
        let config = ReplConfig::new(df_path.clone());
        let mut state = PreviewState::default();

        // First reload — should see A=first.
        let (r1, _, _) = run_reload(&mut state, &config);
        assert!(r1.is_ok());
        assert_eq!(
            state.env.get("A").map(String::as_str),
            Some("first"),
            "after v1 reload, env[A] must be 'first'"
        );

        // Overwrite the file with v2 — ENV B=second (A is gone).
        let mut f2 = std::fs::File::create(&df_path).expect("create v2");
        write!(f2, "FROM scratch\nENV B=second\n").expect("write v2");

        // Second reload — should see B=second and A gone.
        let (r2, _, _) = run_reload(&mut state, &config);
        assert!(r2.is_ok());
        assert!(
            state.env.contains_key("B"),
            "after v2 reload, env must contain B"
        );
        assert!(
            !state.env.contains_key("A"),
            "after v2 reload, env must not contain A"
        );
    }

    // --- reload_engine_error_keeps_old_state ---
    //
    // Trigger EngineError::Io by making a COPY source path unreadable (not just
    // absent — a missing source becomes Warning::MissingCopySource, not an error).
    // We lock a directory with mode 000 so fs::metadata on its contents returns
    // PermissionDenied, which is not NotFound and therefore triggers EngineError::Io.
    //
    // Only runs on Unix because Windows file permissions work differently.

    #[test]
    #[cfg(unix)]
    fn reload_engine_error_keeps_old_state() {
        use std::os::unix::fs::PermissionsExt;

        let ctx_dir = tempfile::TempDir::new().expect("tempdir");

        // Create a locked sub-directory with a file inside it.
        let locked = ctx_dir.path().join("locked");
        std::fs::create_dir(&locked).expect("create locked dir");
        std::fs::write(locked.join("src.txt"), b"data").expect("write src");
        // Remove all permissions — fs::metadata on contents returns PermissionDenied.
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
            .expect("chmod 000");

        // Dockerfile COPYs from inside the locked directory.
        let mut df_file = NamedTempFile::new().expect("tempfile");
        write!(df_file, "FROM scratch\nCOPY locked/src.txt /dst\n").expect("write df");

        // Build config pointing at the Dockerfile with the temp dir as context.
        let config =
            ReplConfig::with_context(df_file.path().to_path_buf(), ctx_dir.path().to_path_buf());

        let mut state = PreviewState::default();
        state.cwd = PathBuf::from("/original");

        let (result, _out, err) = run_reload(&mut state, &config);

        // Restore permissions before any assertion so TempDir can clean up on failure.
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
            .expect("restore permissions");

        assert!(
            result.is_ok(),
            "execute must return Ok even on engine error; got: {result:?}"
        );
        assert_eq!(
            state.cwd,
            PathBuf::from("/original"),
            "state must be preserved on engine error; got: {}",
            state.cwd.display()
        );
        assert!(
            err.contains("engine error"),
            "stderr must contain 'engine error'; got: {err}"
        );
    }

    // --- reload_warnings_emitted_to_err_writer ---
    //
    // FROM scratch always emits Warning::EmptyBaseImage. After reload, stderr
    // must contain "warning".

    #[test]
    fn reload_warnings_emitted_to_err_writer() {
        let mut file = NamedTempFile::new().expect("tempfile");
        // FROM scratch always triggers EmptyBaseImage warning.
        write!(file, "FROM scratch\n").expect("write");

        let config = ReplConfig::new(file.path().to_path_buf());
        let mut state = PreviewState::default();

        let (result, _out, err) = run_reload(&mut state, &config);
        assert!(result.is_ok());
        assert!(
            err.contains("warning"),
            "stderr must contain 'warning' from EmptyBaseImage; got: {err}"
        );
    }

    // --- reload_with_stage_selection_restores_same_stage ---
    //
    // Two-stage Dockerfile: builder (ENV BUILD=1) and final (ENV FINAL=1).
    // When config.selected_stage is "builder", reload must restore the builder
    // stage state (BUILD=1, no FINAL=1), not the final stage.

    #[test]
    fn reload_with_stage_selection_restores_same_stage() {
        let mut file = NamedTempFile::new().expect("tempfile");
        write!(
            file,
            "FROM scratch AS builder\nENV BUILD=1\nFROM scratch\nENV FINAL=1\n"
        )
        .expect("write");

        let config = ReplConfig::new(file.path().to_path_buf())
            .with_selected_stage(Some("builder".to_string()));
        let mut state = PreviewState::default();

        let (result, out, _err) = run_reload(&mut state, &config);
        assert!(result.is_ok(), "execute must return Ok; got: {result:?}");
        assert!(
            out.contains("Reloaded."),
            "stdout must contain 'Reloaded.'; got: {out}"
        );
        assert_eq!(
            state.env.get("BUILD").map(String::as_str),
            Some("1"),
            "state must be the builder stage (BUILD=1)"
        );
        assert!(
            !state.env.contains_key("FINAL"),
            "state must NOT be the final stage (no FINAL key)"
        );
    }

    // --- reload_with_stage_selection_warns_when_stage_removed ---
    //
    // First load has a two-stage Dockerfile with "builder". After overwriting the
    // file to a single-stage Dockerfile, reload must emit a warning that "builder"
    // no longer exists and fall back to the final stage.

    #[test]
    fn reload_with_stage_selection_warns_when_stage_removed() {
        let mut file = NamedTempFile::new().expect("tempfile");
        // v1: two-stage, "builder" exists.
        write!(
            file,
            "FROM scratch AS builder\nENV BUILD=1\nFROM scratch\nENV FINAL=1\n"
        )
        .expect("write v1");

        let df_path = file.path().to_path_buf();
        let config =
            ReplConfig::new(df_path.clone()).with_selected_stage(Some("builder".to_string()));
        let mut state = PreviewState::default();

        // First reload succeeds with builder stage.
        let (r1, _, _) = run_reload(&mut state, &config);
        assert!(r1.is_ok());
        assert!(state.env.contains_key("BUILD"), "v1: must have BUILD");

        // Overwrite with a single-stage Dockerfile — "builder" is gone.
        let mut f2 = std::fs::File::create(&df_path).expect("create v2");
        write!(f2, "FROM scratch\nENV ONLY=1\n").expect("write v2");

        // Second reload: "builder" not found — warning emitted, final stage used.
        let (r2, _out, err) = run_reload(&mut state, &config);
        assert!(
            r2.is_ok(),
            "execute must return Ok on missing stage; got: {r2:?}"
        );
        assert!(
            err.contains("no longer exists after reload"),
            "stderr must warn about missing stage; got: {err}"
        );
        assert_eq!(
            state.env.get("ONLY").map(String::as_str),
            Some("1"),
            "state must fall back to the final stage (ONLY=1)"
        );
    }

    // --- reload_with_context_and_stage_selection_restores_stage ---
    //
    // `ReplConfig::with_context` is used by tests that supply an explicit
    // context directory (e.g. the engine-error test). Verify that chaining
    // `.with_selected_stage` on a `with_context` config correctly re-applies
    // the stage selection on reload, so the two constructors are equivalent
    // from the reload perspective.

    #[test]
    fn reload_with_context_and_stage_selection_restores_stage() {
        let ctx_dir = tempfile::TempDir::new().expect("tempdir");
        let mut file = NamedTempFile::new().expect("tempfile");
        write!(
            file,
            "FROM scratch AS builder\nENV CTX_BUILD=1\nFROM scratch\nENV CTX_FINAL=1\n"
        )
        .expect("write");

        let config =
            ReplConfig::with_context(file.path().to_path_buf(), ctx_dir.path().to_path_buf())
                .with_selected_stage(Some("builder".to_string()));
        let mut state = PreviewState::default();

        let (result, out, _err) = run_reload(&mut state, &config);
        assert!(result.is_ok(), "execute must return Ok; got: {result:?}");
        assert!(
            out.contains("Reloaded."),
            "stdout must contain 'Reloaded.'; got: {out}"
        );
        assert_eq!(
            state.env.get("CTX_BUILD").map(String::as_str),
            Some("1"),
            "state must be builder stage (CTX_BUILD=1)"
        );
        assert!(
            !state.env.contains_key("CTX_FINAL"),
            "state must NOT be final stage"
        );
    }

    // ── Compose mode reload tests ─────────────────────────────────────────────

    /// Build a minimal `ComposeFile` with a single build service.
    fn make_compose_file_with_service(service_name: &str) -> ComposeFile {
        use crate::model::compose::{BuildConfig, Service, VolumeDefinition};
        use std::collections::BTreeMap;
        let service = Service {
            name: service_name.to_string(),
            build: Some(BuildConfig {
                context: std::path::PathBuf::from("."),
                dockerfile: std::path::PathBuf::from("Dockerfile"),
            }),
            image: None,
            environment: vec![],
            env_file: vec![],
            volumes: vec![],
            working_dir: None,
            depends_on: vec![],
            ports: vec![],
        };
        let mut services = BTreeMap::new();
        services.insert(service_name.to_string(), service);
        ComposeFile {
            services,
            volumes: BTreeMap::new(),
        }
    }

    // --- reload_compose_mode_replaces_state ---
    //
    // Compose mode reload: after a reload, the state reflects the env vars
    // set in the Dockerfile referenced by the compose file.

    #[test]
    fn reload_compose_mode_replaces_state() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(
            dir.path().join("Dockerfile"),
            b"FROM scratch\nENV RELOAD_VAR=ok\n",
        )
        .expect("write Dockerfile");

        let compose_yaml = format!(
            "services:\n  api:\n    build:\n      context: .\n      dockerfile: Dockerfile\n    environment:\n      - COMPOSE_VAR=hello\n"
        );
        let compose_path = dir.path().join("compose.yaml");
        std::fs::write(&compose_path, compose_yaml.as_bytes()).expect("write compose.yaml");

        let ctx = ComposeContext {
            compose_file: make_compose_file_with_service("api"),
            selected_service: "api".to_string(),
        };
        let config = ReplConfig::with_context(compose_path, dir.path().to_path_buf())
            .with_compose_context(Some(ctx));
        let mut state = PreviewState::default();

        let (result, out, _err) = run_reload(&mut state, &config);
        assert!(result.is_ok(), "execute must return Ok; got: {result:?}");
        assert!(
            out.contains("Reloaded."),
            "stdout must contain 'Reloaded.'; got: {out}"
        );
        assert_eq!(
            state.env.get("COMPOSE_VAR").map(String::as_str),
            Some("hello"),
            "state must contain COMPOSE_VAR=hello from compose environment"
        );
    }

    // --- reload_compose_mode_parse_error_keeps_old_state ---
    //
    // If the compose file is invalid YAML on reload, old state is preserved.

    #[test]
    fn reload_compose_mode_parse_error_keeps_old_state() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let compose_path = dir.path().join("compose.yaml");
        std::fs::write(&compose_path, b"{ not: valid: yaml: [\n").expect("write bad yaml");

        let ctx = ComposeContext {
            compose_file: make_compose_file_with_service("api"),
            selected_service: "api".to_string(),
        };
        let config = ReplConfig::with_context(compose_path, dir.path().to_path_buf())
            .with_compose_context(Some(ctx));
        let mut state = PreviewState::default();
        state.cwd = PathBuf::from("/original");

        let (result, _out, err) = run_reload(&mut state, &config);
        assert!(
            result.is_ok(),
            "execute must return Ok on parse error; got: {result:?}"
        );
        assert_eq!(
            state.cwd,
            PathBuf::from("/original"),
            "state must be preserved on parse error"
        );
        assert!(
            err.contains("parse error") || err.contains("compose"),
            "stderr must mention compose parse error; got: {err}"
        );
    }

    // --- reload_compose_mode_emits_warnings ---
    //
    // An image-only service emits ImageOnlyService warning on reload.

    #[test]
    fn reload_compose_mode_emits_warnings() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let compose_path = dir.path().join("compose.yaml");
        let compose_yaml = "services:\n  db:\n    image: postgres:15\n";
        std::fs::write(&compose_path, compose_yaml.as_bytes()).expect("write compose");

        let ctx = ComposeContext {
            compose_file: make_compose_file_with_service("db"),
            selected_service: "db".to_string(),
        };
        let config = ReplConfig::with_context(compose_path, dir.path().to_path_buf())
            .with_compose_context(Some(ctx));
        let mut state = PreviewState::default();

        let (result, _out, err) = run_reload(&mut state, &config);
        assert!(result.is_ok());
        assert!(
            err.contains("warning:"),
            "stderr must contain 'warning:' for ImageOnlyService; got: {err}"
        );
        assert!(
            err.contains("filesystem is empty"),
            "ImageOnlyService warning must say 'filesystem is empty'; got: {err}"
        );
    }
}
