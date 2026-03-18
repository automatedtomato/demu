//! Applies parsed Dockerfile instructions into preview state.
//!
//! The single public entry point is [`run`], which walks a `Vec<Instruction>`
//! and returns a fully-populated [`PreviewState`].

mod copy;
pub mod error;
mod run_sim;
mod runner;

pub use error::EngineError;
pub use runner::run;
