/// Smoke tests that verify the module structure of the demu crate.
/// These tests intentionally fail to compile until the scaffold is in place.
use demu::engine::EngineError;
use demu::explain::Explain;
use demu::model::PreviewState;
use demu::parser::ParseError;
use demu::repl::Repl;

#[test]
fn model_preview_state_is_accessible() {
    let _state = PreviewState;
}

#[test]
fn parser_parse_error_is_accessible() {
    // Verify ParseError is a public, Debug-able type in the parser module.
    let _e: Option<ParseError> = None;
}

#[test]
fn engine_engine_error_is_accessible() {
    // Verify EngineError is a public type in the engine module.
    let _e: Option<EngineError> = None;
}

#[test]
fn repl_repl_is_accessible() {
    let _r: Option<Repl> = None;
}

#[test]
fn explain_explain_is_accessible() {
    let _x: Option<Explain> = None;
}
