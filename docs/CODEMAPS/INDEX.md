# demu Codemap Index

**Last Updated:** 2026-03-17

## Current state

Rust crate scaffold complete with full domain model (`src/model/`). The model layer defines all core types used throughout the engine:
virtual filesystem (FileNode, DirNode, SymlinkNode), environment state, layer history, and provenance tracking.
CLI parses arguments correctly. Parser, engine, and REPL remain as stubs.

## Module map

| Module | File | Purpose | Status |
|---|---|---|---|
| `cli` | `src/cli.rs` | Parses `-f`/`--file` and `--stage` flags via clap | Stub — arg parsing works, no dispatch |
| `parser` | `src/parser/mod.rs` | Turns Dockerfile/Compose files into typed instruction models | Stub — `ParseError` placeholder only |
| `model` | `src/model/` (5 submodules) | Typed domain model: virtual filesystem, env, layers, provenance | Complete — all types defined with 89 tests |
| `engine` | `src/engine/mod.rs` | Applies parsed instructions into preview state | Stub — `EngineError` placeholder only |
| `repl` | `src/repl/mod.rs` | Interactive shell loop over preview state | Stub — `Repl` unit struct only |
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

**89 tests pass; zero clippy warnings.**

Key test groups:

- `tests/scaffold.rs` — 8 integration tests for CLI, parser, engine, repl, explain
- `src/model/fs.rs` — 40 unit tests for VirtualFs, filesystem operations, immutability
- `src/model/state.rs` — 23 unit tests for PreviewState, InstalledRegistry, history tracking
- `src/model/provenance.rs` — 10 unit tests for Provenance, ProvenanceSource
- `src/model/instruction.rs` — 8 unit tests for Instruction, CopySource enums

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
