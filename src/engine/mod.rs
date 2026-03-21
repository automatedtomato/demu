//! Applies parsed Dockerfile instructions into preview state.
//!
//! The single public entry point is [`run`], which walks a `Vec<Instruction>`
//! and returns an [`EngineOutput`] containing the final stage's [`PreviewState`]
//! and a [`StageRegistry`] for all stages.
//!
//! The compose engine entry point is [`compose::run_compose`], which builds a
//! merged `PreviewState` from a parsed `ComposeFile` and a selected service.

pub mod compose;
mod copy;
pub mod error;
pub mod mount;
mod run_sim;
mod runner;

pub use compose::{run_compose, ComposeEngineError, ComposeEngineOutput};
pub use error::EngineError;
pub use runner::{run, EngineOutput};
// Re-export StageRegistry from the model so callers can use engine::StageRegistry
// without needing to import from model directly.
pub use crate::model::state::StageRegistry;
