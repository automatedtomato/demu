pub mod engine;
pub mod explain;
pub mod model;
pub mod parser;
pub mod repl;

// Re-export Cli so integration tests can test argument parsing without
// going through the binary entrypoint.
pub use cli::Cli;
mod cli;
