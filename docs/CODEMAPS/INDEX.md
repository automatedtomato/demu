# demu Codemap Index

**Last Updated:** 2026-03-21 (after v0.4.0 — Compose service preview)

## Current state

Rust crate complete with full domain model (`src/model/`), Dockerfile parser (`src/parser/`), Compose parser (`src/parser/compose.rs`), engine (`src/engine/`) with Compose merge and mount shadow support, and fully interactive REPL (`src/repl/`) with Compose-specific commands. The model layer defines all core types; the parser converts Dockerfile and Compose YAML files into typed instruction models; the engine applies those instructions and merges service configuration into a preview state; the REPL provides an interactive shell for exploring that state, with Compose-specific commands (`:services`, `:mounts`, `:depends`).
CLI fully parses `-f`, `--stage`, `--compose`, and `--service` flags via clap. The explain module remains as a stub.

## Module map

| Module | File | Purpose | Status |
|---|---|---|---|
| `cli` | `src/cli.rs` | Parses `-f`/`--file`, `--stage`, `--compose`, and `--service` flags via clap | Complete — full flag parsing and validation (v0.4.0) |
| `parser` | `src/parser/` (3 submodules) | Turns Dockerfile and Compose YAML files into typed instruction models | Complete — hand-rolled Dockerfile parser + serde-yaml Compose parser with 19+ inline unit tests and 10+ fixture-based integration tests |
| `model` | `src/model/` (5 submodules) | Typed domain model: virtual filesystem, env, layers, provenance | Complete — all types defined with 89 tests |
| `engine` | `src/engine/` (5+ submodules) | Applies parsed instructions into preview state; handles COPY, RUN, Compose merge, and mount shadowing | Complete — handles COPY with recursive dirs, RUN simulation, Compose service merge, volume shadow model; 10+ integration tests (v0.4.0) |
| `repl` | `src/repl/` (10+ files) | Interactive shell loop: rustyline, command parsing, dispatch, path resolution, 8 standard commands, 3 Compose-specific commands | Complete — 14+ files with 60+ tests covering all shell commands and Compose commands (`:services`, `:mounts`, `:depends`) |
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

## Parser submodules (Issue #3, extended in v0.4.0)

The `src/parser/` directory contains three typed submodules:

| Submodule | Key Types | Purpose |
|-----------|-----------|---------|
| `error.rs` | `ParseError` enum | Parse errors with line numbers and descriptive messages via `thiserror` |
| `dockerfile.rs` | `Instruction` enum (parser-facing) | Hand-rolled line-based Dockerfile parser; exports `parse_dockerfile(&str) -> Result<Vec<Instruction>, ParseError>` |
| `compose.rs` | `ComposeFile`, `Service` structs (v0.4.0) | serde-yaml-based Compose file parser; handles service definitions, environment, volumes, working_dir, depends_on with path traversal safety checks |

The Dockerfile parser handles: `FROM`, `RUN`, `COPY`, `ENV`, `WORKDIR`, `USER`, `EXPOSE`, `ENTRYPOINT`, `CMD`, plus comments and empty lines.
Fully tested with 19+ inline unit tests and 10+ fixture-based integration tests using `.dockerfile` and `.yaml` files.

## Engine submodules (Issue #4, extended in v0.4.0)

The `src/engine/` directory contains 5+ typed submodules:

| Submodule | Key Types | Purpose |
|-----------|-----------|---------|
| `error.rs` | `EngineError` enum | Engine errors (I/O, missing sources, invalid state) via `thiserror` |
| `runner.rs` | `pub fn run(Vec<Instruction>, &Path) -> Result<PreviewState, EngineError>` | Main orchestrator: applies instructions sequentially, maintains immutable preview state |
| `copy.rs` | `fn apply_copy(...)` | Handles COPY instruction: supports recursive directories, issues warnings for missing sources |
| `run_sim.rs` | `fn apply_run(...)` | Handles RUN instruction: records command and warning (simulates without host execution) |
| `compose.rs` (v0.4.0) | `fn merge_service(...)` | Merges Compose service configuration into Dockerfile preview: environment inheritance, working_dir override, volume mount shadows, path traversal containment |

The engine applies parsed instructions to build a preview state. COPY recursively copies files and directories with provenance tracking. RUN records commands without executing them on the host. Compose merge applies service-level overrides and shadow model for mounts (v0.4.0). Fully tested with 10+ integration tests using real fixture Dockerfiles, Compose files, and context directories.

## REPL submodules (Issue #5, extended in v0.4.0)

The `src/repl/` directory contains 14+ files implementing a fully interactive shell:

| File | Key Types/Functions | Purpose |
|------|---------------------|---------|
| `mod.rs` | `pub fn run_repl()`, `pub fn dispatch()`, `Repl` struct | Main REPL loop: rustyline editor, prompt management, command dispatch; 30+ integration tests (v0.4.0) |
| `error.rs` | `ReplError` enum | REPL errors (path not found, unknown command, I/O) via `thiserror`; `ReplError::Io` variant added v0.4.0 |
| `parse.rs` | `ParsedCommand` enum, `pub fn parse_input(&str)` | Input parsing: converts raw text to typed commands; handles flags (`-l`, `-la`), arguments, and patterns; 50+ unit tests |
| `path.rs` | `pub fn resolve_path()` | Pure path arithmetic: resolves absolute/relative paths, handles `..` and `.`, never touches filesystem |
| `commands/ls.rs` | `pub fn execute()` | Lists directory contents; supports `-l`/`-la` long format; 8 tests |
| `commands/cd.rs` | `pub fn execute()` | Changes working directory; validates path existence; 4 tests |
| `commands/pwd.rs` | `pub fn execute()` | Prints current working directory |
| `commands/cat.rs` | `pub fn execute()` | Prints file contents; 5 tests |
| `commands/find.rs` | `pub fn execute()` | Recursive filesystem search with glob pattern support (`*.rs`, `?.txt`); iterative DP traversal; 6 tests |
| `commands/env_cmd.rs` | `pub fn execute()` | Prints environment variables in sorted order; 3 tests |
| `commands/help.rs` | `pub fn execute()` | Displays command reference |
| `commands/services.rs` (v0.4.0) | `pub fn execute()` | Lists all Compose services and their status (Compose mode only) |
| `commands/mounts.rs` (v0.4.0) | `pub fn execute()` | Shows volume mount configuration and shadow explanations (Compose mode) |
| `commands/depends.rs` (v0.4.0) | `pub fn execute()` | Shows service dependency tree with diamond deduplication (Compose mode) |
| `commands/mod.rs` | Module declarations | Declares all command submodules |

The REPL provides a lightweight, testable shell over the preview state. Command parsing is pure and infallible (unknown input becomes `Unknown` variant, not an error). All handlers are fully tested with 60+ unit and integration tests. Path resolution handles both absolute and relative paths without touching the real filesystem. Compose-specific commands (v0.4.0) only active in `--compose` mode.

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

**320+ tests pass; zero clippy warnings.** (125 new tests added in Issue #5 for REPL; 30+ additional tests added in v0.4.0 for Compose support)

Key test groups:

- `tests/scaffold.rs` — 10+ integration tests for CLI, parser, engine, repl, explain
- `tests/parser_fixtures.rs` — 10+ fixture-based integration tests using `.dockerfile` and `.yaml` files
- `tests/engine_integration.rs` — 10+ fixture-based integration tests for COPY, RUN, Compose merge, and state mutation
- `tests/compose_*.rs` — 10+ tests for Compose service merge, mount shadows, and security containment (v0.4.0)
- `src/parser/dockerfile.rs` — 19 inline unit tests for instruction parsing
- `src/parser/compose.rs` — 15+ unit tests for Compose YAML parsing (v0.4.0)
- `src/model/fs.rs` — 40 unit tests for VirtualFs, filesystem operations, immutability
- `src/model/state.rs` — 23 unit tests for PreviewState, InstalledRegistry, history tracking
- `src/model/provenance.rs` — 10 unit tests for Provenance, ProvenanceSource
- `src/model/instruction.rs` — 8 unit tests for Instruction, CopySource enums
- `src/repl/mod.rs` — 30+ integration tests for REPL dispatch and command integration (v0.4.0)
- `src/repl/parse.rs` — 50+ unit tests for input parsing (ls, cd, pwd, cat, find, env, exit, help, unknown, empty)
- `src/repl/commands/ls.rs` — 8 tests for directory listing
- `src/repl/commands/cd.rs` — 4 tests for directory changes
- `src/repl/commands/cat.rs` — 5 tests for file output
- `src/repl/commands/find.rs` — 6 tests for recursive search
- `src/repl/commands/env_cmd.rs` — 3 tests for environment variables
- `src/repl/commands/services.rs` — 5+ tests for service listing (v0.4.0)
- `src/repl/commands/mounts.rs` — 5+ tests for mount display (v0.4.0)
- `src/repl/commands/depends.rs` — 5+ tests for dependency tree (v0.4.0)
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
