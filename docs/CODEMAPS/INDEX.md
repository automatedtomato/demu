# demu Codemap Index

**Last Updated:** 2026-03-17

## Current state

Rust crate scaffold only. All five modules exist as stubs with placeholder types.
The CLI (`demu -f <path> [--stage <name>]`) parses arguments correctly but produces
no preview output. No Dockerfile parsing, virtual filesystem, or REPL is implemented.

## Module map

| Module | File | Purpose | Status |
|---|---|---|---|
| `cli` | `src/cli.rs` | Parses `-f`/`--file` and `--stage` flags via clap | Stub — arg parsing works, no dispatch |
| `parser` | `src/parser/mod.rs` | Turns Dockerfile/Compose files into typed instruction models | Stub — `ParseError` placeholder only |
| `model` | `src/model/mod.rs` | Typed domain model: virtual filesystem, env, layers, provenance | Stub — `PreviewState` unit struct only |
| `engine` | `src/engine/mod.rs` | Applies parsed instructions into preview state | Stub — `EngineError` placeholder only |
| `repl` | `src/repl/mod.rs` | Interactive shell loop over preview state | Stub — `Repl` unit struct only |
| `explain` | `src/explain/mod.rs` | Answers provenance questions about files and instructions | Stub — `Explain` unit struct only |
| entrypoint | `src/main.rs` | Binary entrypoint; calls `Cli::parse()` and exits | Stub — prints "preview not yet implemented" |
| lib root | `src/lib.rs` | Re-exports `Cli` and declares all five modules | Complete for current scope |

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

`tests/scaffold.rs` — 8 integration tests:

- `model_preview_state_is_accessible`
- `parser_parse_error_is_constructible_and_displayable`
- `engine_engine_error_is_constructible_and_displayable`
- `repl_repl_is_constructible`
- `explain_explain_is_constructible`
- `cli_accepts_file_argument`
- `cli_accepts_stage_argument`
- `cli_rejects_missing_file_argument`

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
