/// Smoke tests that verify the module structure of the demu crate.
/// Each test constructs or exercises the placeholder type so that
/// structural breakage (missing derives, moved types, API changes)
/// causes a runtime failure, not just a compile error.
use demu::engine::EngineError;
use demu::explain::Explain;
use demu::model::PreviewState;
use demu::parser::ParseError;
use demu::repl::Repl;

#[test]
fn model_preview_state_is_accessible() {
    let _state = PreviewState::default();
}

#[test]
fn parser_parse_error_is_constructible_and_displayable() {
    let e = ParseError::InvalidInstruction {
        line: 1,
        message: "test error".to_string(),
    };
    let msg = format!("{e}");
    assert!(
        !msg.is_empty(),
        "ParseError Display impl must produce a non-empty message"
    );
}

#[test]
fn engine_engine_error_is_constructible_and_displayable() {
    let e = EngineError;
    let msg = format!("{e}");
    assert!(
        !msg.is_empty(),
        "EngineError Display impl must produce a non-empty message"
    );
}

#[test]
fn repl_repl_is_constructible() {
    let _r = Repl;
}

#[test]
fn explain_explain_is_constructible() {
    let _x = Explain;
}

#[test]
fn cli_accepts_file_argument() {
    // Verify the CLI parses -f/--file correctly using clap's test helper.
    use clap::Parser;
    use demu::Cli;

    let cli =
        Cli::try_parse_from(["demu", "-f", "Dockerfile"]).expect("CLI should accept -f <path>");
    assert_eq!(cli.file, std::path::PathBuf::from("Dockerfile"));
    assert!(cli.stage.is_none());
}

#[test]
fn cli_accepts_stage_argument() {
    use clap::Parser;
    use demu::Cli;

    let cli = Cli::try_parse_from(["demu", "-f", "Dockerfile", "--stage", "builder"])
        .expect("CLI should accept --stage");
    assert_eq!(cli.stage.as_deref(), Some("builder"));
}

#[test]
fn cli_rejects_missing_file_argument() {
    use clap::Parser;
    use demu::Cli;

    let result = Cli::try_parse_from(["demu"]);
    assert!(result.is_err(), "CLI must require -f/--file");
}
