//! Applies parsed Dockerfile instructions into preview state.
//!
//! The single public entry point is [`run`], which walks a `Vec<Instruction>`
//! and returns an [`EngineOutput`] containing the final stage's [`PreviewState`]
//! and a [`StageRegistry`] for all stages.

mod copy;
pub mod error;
mod run_sim;
mod runner;

pub use error::EngineError;
pub use runner::{run, EngineOutput};
// Re-export StageRegistry from the model so callers can use engine::StageRegistry
// without needing to import from model directly.
pub use crate::model::state::StageRegistry;
