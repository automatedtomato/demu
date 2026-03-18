# demu Codemap Index

**Last Updated:** 2026-03-18 (after Issue #5 completion)

## Current state

Rust crate complete with full domain model (`src/model/`), Dockerfile parser (`src/parser/`), engine (`src/engine/`), and fully interactive REPL (`src/repl/`). The model layer defines all core types; the parser converts Dockerfile/Compose files into typed instruction models; the engine applies those instructions to build a preview state; the REPL provides an interactive shell for exploring that state.
CLI parses arguments correctly. The explain module remains as a stub.

## Module map

| Module | File | Purpose | Status |
|---|---|---|---|
| `cli` | `src/cli.rs` | Parses `-f`/`--file` and `--stage` flags via clap | Stub — arg parsing works, no dispatch |
| `parser` | `src/parser/` (2 submodules) | Turns Dockerfile/Compose files into typed instruction models | Complete — hand-rolled line-based parser with 19 inline unit tests and 8 fixture-based integration tests |
| `model` | `src/model/` (5 submodules) | Typed domain model: virtual filesystem, env, layers, provenance | Complete — all types defined with 89 tests |
| `engine` | `src/engine/` (4 submodules) | Applies parsed instructions into preview state; handles COPY, RUN, and other operations | Complete — handles COPY with recursive dirs, RUN simulation with warnings; 6 integration tests |
| `repl` | `src/repl/` (8 files) | Interactive shell loop: rustyline, command parsing, dispatch, path resolution, 8 standard commands | Complete — 12 files with 48+ tests covering pwd, env, help, ls, cat, cd, find, exit |
| `explain` | `src/explain/mod.rs` | Answers provenance questions about files and instructions | Stub — `Explain` unit struct only |
| entrypoint | `src/main.rs` | Binary entrypoint; calls `Cli::parse()` and exits | Stub — prints "preview not yet implemented" |
| lib root | `src/lib.rs` | Re-exports `Cli` and declares all five modules | Complete for current scope |

## Model submodules (Issue #2)

The `src/model/` directory contains five typed submodules:

| Submodule | Types | Purpose |
|-----------|-------|---------|
| `provenance.rs` | `Provenance`, `ProvenanceSource`, `MountInfo` | Tracks where files came from (COPY, RUN, mount, base image) |
| `warning.rs` | `Warning` enum | Non-fatal diagnostics (unsupported commands, skipped behaviors) |
| `instruction.rs` | `Instruction` enum, `CopySource` enum | Parsed Dockerfile instructions (FROM, RUN, COPY, ENV, etc.) |
| `fs.rs` | `FileNode`, `DirNode`, `SymlinkNode`, `FsNode`, `VirtualFs` | Virtual filesystem representation with immutable tree updates |
| `state.rs` | `PreviewState`, `InstalledRegistry`, `HistoryEntry`, `LayerSummary` | Complete preview state: filesystem, environment, history, installed packages |

All types are immutable, fully tested (89 tests), and documented with rustdoc.

## Parser submodules (Issue #3)

The `src/parser/` directory contains two typed submodules:

| Submodule | Key Types | Purpose |
|-----------|-----------|---------|
| `error.rs` | `ParseError` enum | Parse errors with line numbers and descriptive messages via `thiserror` |
| `dockerfile.rs` | `Instruction` enum (parser-facing) | Hand-rolled line-based Dockerfile parser; exports `parse_dockerfile(&str) -> Result<Vec<Instruction>, ParseError>` |

The parser handles v0.1 subset: `FROM`, `RUN`, `COPY`, `ENV`, `WORKDIR`, `USER`, `EXPOSE`, `ENTRYPOINT`, `CMD`, plus comments and empty lines.
Fully tested with 19 inline unit tests and 8 fixture-based integration tests using `.dockerfile` files.

## Engine submodules (Issue #4)

The `src/engine/` directory contains four typed submodules:

| Submodule | Key Types | Purpose |
|-----------|-----------|---------|
| `error.rs` | `EngineError` enum | Engine errors (I/O, missing sources, invalid state) via `thiserror` |
| `runner.rs` | `pub fn run(Vec<Instruction>, &Path) -> Result<PreviewState, EngineError>` | Main orchestrator: applies instructions sequentially, maintains immutable preview state |
| `copy.rs` | `fn apply_copy(...)` | Handles COPY instruction: supports recursive directories, issues warnings for missing sources |
| `run_sim.rs` | `fn apply_run(...)` | Handles RUN instruction: records command and warning (simulates without host execution) |

The engine applies parsed instructions to build a preview state. COPY recursively copies files and directories with provenance tracking. RUN records commands without executing them on the host. Fully tested with 6 integration tests using real fixture Dockerfiles and context directories.

## REPL submodules (Issue #5)

The `src/repl/` directory contains eight files implementing a fully interactive shell:

| File | Key Types/Functions | Purpose |
|------|---------------------|---------|
| `mod.rs` | `pub fn run_repl()`, `pub fn dispatch()`, `Repl` struct | Main REPL loop: rustyline editor, prompt management, command dispatch; 28 integration tests |
| `error.rs` | `ReplError` enum | REPL errors (path not found, unknown command, etc.) via `thiserror` |
| `parse.rs` | `ParsedCommand` enum, `pub fn parse_input(&str)` | Input parsing: converts raw text to typed commands; handles flags (`-l`, `-la`), arguments, and patterns; 50+ unit tests |
| `path.rs` | `pub fn resolve_path()` | Pure path arithmetic: resolves absolute/relative paths, handles `..` and `.`, never touches filesystem |
| `commands/ls.rs` | `pub fn execute()` | Lists directory contents; supports `-l`/`-la` long format; 8 tests |
| `commands/cd.rs` | `pub fn execute()` | Changes working directory; validates path existence; 4 tests |
| `commands/pwd.rs` | `pub fn execute()` | Prints current working directory |
| `commands/cat.rs` | `pub fn execute()` | Prints file contents; 5 tests |
| `commands/find.rs` | `pub fn execute()` | Recursive filesystem search with glob pattern support (`*.rs`, `?.txt`); iterative DP traversal; 6 tests |
| `commands/env_cmd.rs` | `pub fn execute()` | Prints environment variables in sorted order; 3 tests |
| `commands/help.rs` | `pub fn execute()` | Displays command reference |
| `commands/mod.rs` | Module declarations | Declares all command submodules |

The REPL provides a lightweight, testable shell over the preview state. Command parsing is pure and infallible (unknown input becomes `Unknown` variant, not an error). All handlers are fully tested with 48+ unit and integration tests. Path resolution handles both absolute and relative paths without touching the real filesystem.

## Data flow (planned)

From `docs/02-architecture.md`:

```text
CLI input
  -> parse Dockerfile / Compose        (parser)
  -> build internal model              (model)
  -> apply instructions into state     (engine)
  -> open REPL over preview state      (repl)
  -> answer shell and custom commands  (repl + explain)
```

## Dev environment

### With Docker (no local Rust required)

```bash
docker compose -f docker-compose.dev.yml run --rm dev bash
cargo build
cargo test
```

### With local Rust (stable toolchain)

```bash
cargo build
cargo test
```

Toolchain is pinned to stable via `rust-toolchain.toml`.

## Test coverage

**287 tests pass; zero clippy warnings.** (125 new tests added in Issue #5 for REPL)

Key test groups:

- `tests/scaffold.rs` — 8 integration tests for CLI, parser, engine, repl, explain
- `tests/parser_fixtures.rs` — 8 fixture-based integration tests using `.dockerfile` files
- `tests/engine_integration.rs` — 6 fixture-based integration tests for COPY, RUN, and state mutation
- `src/parser/dockerfile.rs` — 19 inline unit tests for instruction parsing
- `src/model/fs.rs` — 40 unit tests for VirtualFs, filesystem operations, immutability
- `src/model/state.rs` — 23 unit tests for PreviewState, InstalledRegistry, history tracking
- `src/model/provenance.rs` — 10 unit tests for Provenance, ProvenanceSource
- `src/model/instruction.rs` — 8 unit tests for Instruction, CopySource enums
- `src/repl/mod.rs` — 28 integration tests for REPL dispatch and command integration
- `src/repl/parse.rs` — 50+ unit tests for input parsing (ls, cd, pwd, cat, find, env, exit, help, unknown, empty)
- `src/repl/commands/ls.rs` — 8 tests for directory listing
- `src/repl/commands/cd.rs` — 4 tests for directory changes
- `src/repl/commands/cat.rs` — 5 tests for file output
- `src/repl/commands/find.rs` — 6 tests for recursive search
- `src/repl/commands/env_cmd.rs` — 3 tests for environment variables
- Engine unit tests — embedded in `src/engine/*.rs` modules, covering error handling and instruction application

## Key files

| File | Purpose |
|---|---|
| `Cargo.toml` | Crate metadata; deps: clap 4, rustyline 14, thiserror 2, anyhow 1 |
| `rust-toolchain.toml` | Pins toolchain to stable |
| `rustfmt.toml` | Code formatting config |
| `Dockerfile.dev` | Dev container image definition |
| `docker-compose.dev.yml` | Dev environment launcher |
| `.github/workflows/ci.yml` | CI pipeline |

## Related docs

- [Product](../01-product.md)
- [Architecture](../02-architecture.md)
- [CLI and REPL](../03-cli-and-repl.md)
- [Dockerfile Semantics](../04-dockerfile-semantics.md)
- [RUN Simulation](../05-run-simulation.md)
- [Compose Plan](../06-compose-plan.md)
- [Roadmap](../07-roadmap.md)
- [Test Strategy](../08-test-strategy.md)
