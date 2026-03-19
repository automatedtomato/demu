// Custom REPL inspection command handlers.
//
// Each submodule implements a colon-prefixed command that reads from
// `PreviewState` and writes a formatted report to an `impl Write` sink.
// None of these handlers mutate `PreviewState` — they are pure read commands.

pub mod history;
pub mod installed;
pub mod layers;
