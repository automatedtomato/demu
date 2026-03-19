// Custom REPL inspection command handlers.
//
// Each submodule implements a colon-prefixed command that reads from (or
// updates) `PreviewState` and writes a formatted report to an `impl Write`
// sink.
//
// `reload` is the only handler here that mutates `PreviewState`; all others
// are pure read commands.

pub mod history;
pub mod installed;
pub mod layers;
pub mod reload;
