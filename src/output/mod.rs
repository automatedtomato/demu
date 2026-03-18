// Output utilities for the demu terminal interface.
//
// This module contains helpers that are shared across the binary entrypoint
// and all presentation layers (REPL, custom commands, future explain/exec
// modes). Placing them here avoids coupling callers to the `repl` module when
// they have no REPL dependency.

pub mod sanitize;
