# Decision 002: Dockerfile Parser Approach

**Status:** Accepted
**Date:** 2026-03-17

## Decision

Write a minimal hand-rolled line-based parser for the v0.1 instruction subset.
Do **not** use an external Dockerfile parser crate for the initial implementation.

## v0.1 supported instructions

`FROM`, `WORKDIR`, `COPY`, `ENV`, `RUN` — 5 instructions only.

## Rationale

- 5 instructions is a trivial parsing surface; a crate brings more complexity than it saves
- Full control over the AST shape means types stay aligned with demu's `model` layer
- External crates (e.g. `dockerfile-parser`) expose their own AST types which would require translation anyway
- Line continuation (`\` at EOL) and here-docs are out of scope for v0.1

## Trade-offs accepted

- No support for multi-line `RUN` with `\` continuation in v0.1
- No validation beyond recognizing the 5 instructions (unknown instructions recorded as `Instruction::Unknown`)

## Future extensibility

If instruction coverage grows significantly (v0.3+), evaluate adopting a proper parser combinator (e.g. `nom` or `winnow`) or contributing to / forking an existing crate. The `parser` module is fully isolated behind a `parse_dockerfile(input: &str) -> Result<Vec<Instruction>, ParseError>` interface, so the implementation can be swapped without touching other layers.
