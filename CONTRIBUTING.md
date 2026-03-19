# Contributing to demu

Thanks for your interest in contributing!

## Before you start

- Check the [open issues](https://github.com/automatedtomato/demu/issues) to see if your idea or bug is already tracked.
- For significant changes, open an issue first to discuss the approach before writing code.

## Development setup

Requires Rust stable.

```bash
git clone https://github.com/automatedtomato/demu.git
cd demu
cargo build
cargo test
```

A containerized environment is also available:

```bash
docker compose -f docker-compose.dev.yml run --rm dev bash
```

## Making changes

1. Fork the repository and create a branch from `main`.
2. Write tests first — see [docs/08-test-strategy.md](docs/08-test-strategy.md).
3. Make sure all tests pass: `cargo test`
4. Make sure clippy is clean: `cargo clippy -- -D warnings`
5. Make sure formatting is correct: `cargo fmt --check`
6. Open a pull request against `main` with a clear description of what and why.

## What demu is (and isn't)

demu is a **preview tool**, not a container runtime. Before adding a feature, check [docs/01-product.md](docs/01-product.md) to understand the product boundary. The guiding principle is:

> Fast, safe, explainable previews over perfect fidelity.

Simulated behavior must always be surfaced via warnings — never silently faked.

## Reporting bugs

Use the [Bug Report](https://github.com/automatedtomato/demu/issues/new?template=bug_report.md) template. Include the Dockerfile snippet that triggers the issue and the output you expected vs. what you got.

## Suggesting features

Use the [Feature Request](https://github.com/automatedtomato/demu/issues/new?template=feature_request.md) template. Features that require a Docker daemon or network access are out of scope for the MVP.

## Code style

- Keep functions short and composable.
- Prefer explicit enums over stringly-typed branching.
- No `unwrap()` or `expect()` in non-test code — use proper error handling.
- See [docs/02-architecture.md](docs/02-architecture.md) for module conventions.
