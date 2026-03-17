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

**Current: scaffold complete — v0.1.0 in progress.**

The Rust crate exists with typed module stubs and a working CLI skeleton. No real preview functionality is implemented yet.

What works today:

- `demu -f <path>` and `demu -f <path> --stage <name>` parse correctly
- All five modules (`parser`, `model`, `engine`, `repl`, `explain`) exist as stubs
- 8 integration tests pass, covering module structure and CLI argument parsing
- `cargo build` and `cargo test` succeed

What does not work yet:

- Dockerfile parsing
- Virtual filesystem
- REPL loop
- Any preview output (binary prints `"demu: preview not yet implemented"`)

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
