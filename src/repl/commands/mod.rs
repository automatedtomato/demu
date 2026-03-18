// Command handler modules for the REPL.
//
// Each submodule exposes a single `execute` function that reads from
// `PreviewState` (or mutates it for `cd`) and writes to an `impl Write`
// output sink. Using a writer rather than printing to stdout directly keeps
// all handlers fully testable without capturing stdout.

pub mod cat;
pub mod cd;
pub mod env_cmd;
pub mod find;
pub mod help;
pub mod ls;
pub mod pwd;
