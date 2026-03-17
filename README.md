# demu

`demu` is a fast preview shell for Dockerfile and Compose files.

It lets you inspect the **filesystem, env, stages, mounts, and simulated installed packages** a config is trying to create, **without fully building or running containers**.

## Why

When you are editing a `Dockerfile` or `compose.yaml`, you often just want to answer questions like:

- What files would be visible?
- What is the working directory?
- What env vars would exist?
- Did this `COPY` land where I think it did?
- Does this image look like it has `curl`, `git`, or `fastapi` installed?

`demu` is for that fast feedback loop.

## Example

```bash
demu -f Dockerfile
demu -f Dockerfile --stage builder
```

Inside the preview shell:

```bash
ls -la
cd /app
cat package.json
find .
env
:layers
:history
:installed
:explain /app/main.py
exit
```

## What it is not

`demu` is **not**:

- a container runtime
- a Docker replacement
- a full shell emulator
- a dependency solver
- a Kubernetes tool

It prefers **fast, safe previews** over perfect fidelity.

## Status

**Current: core engine complete — v0.1.0 in progress (after PR #13, Issue #4).**

The Rust crate has a complete Dockerfile parser, typed domain model, and fully working preview engine. REPL and explain modules remain as stubs.

What works today:

- `demu -f <path>` and `demu -f <path> --stage <name>` parse correctly
- Dockerfile parsing: `FROM`, `RUN`, `COPY`, `ENV`, `WORKDIR`, plus other instructions
- Virtual filesystem with immutable tree updates and provenance tracking
- Engine applies all parsed instructions: COPY reads real files from build context, RUN records commands
- 159 tests pass (89 model, 19 parser, 6 engine integration, 8 CLI scaffold, 8 parser fixtures, plus embedded unit tests)
- `cargo build` and `cargo test` succeed with zero clippy warnings

What does not work yet:

- REPL loop and interactive shell
- `:explain` command (provenance query)
- Any runtime output (binary prints `"demu: preview not yet implemented"`)

The first target is a Dockerfile-focused MVP. Compose support comes after.

## Building and testing

Requires Rust stable (managed via `rust-toolchain.toml`).

```bash
cargo build
cargo test
```

## Dev environment

A containerized dev environment is provided via Docker Compose:

```bash
docker compose -f docker-compose.dev.yml run --rm dev bash
```

Inside the container, `cargo build` and `cargo test` work without a local Rust install.

## Docs

- [Product](./docs/01-product.md)
- [Architecture](./docs/02-architecture.md)
- [CLI and REPL](./docs/03-cli-and-repl.md)
- [Dockerfile Semantics](./docs/04-dockerfile-semantics.md)
- [RUN Simulation](./docs/05-run-simulation.md)
- [Compose Plan](./docs/06-compose-plan.md)
- [Roadmap](./docs/07-roadmap.md)
- [Test Strategy](./docs/08-test-strategy.md)
- [Codemap Index](./docs/CODEMAPS/INDEX.md)
