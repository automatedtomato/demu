# Dockerfile semantics for demu

## Scope

This document defines how `demu` should interpret Dockerfile instructions.

This is a preview model, not Docker's exact runtime behavior.

## Supported instructions for MVP

- `FROM`
- `WORKDIR`
- `COPY`
- `ENV`
- `RUN`

Optional early support if easy:

- `ARG`
- `CMD`
- `ENTRYPOINT`
- `EXPOSE`

## `FROM`

### Expected behavior

- Start a new stage
- Set the current stage
- Record the base image string
- Do not attempt to resolve actual image filesystem for MVP unless an image snapshot source is later added

### Important note

For MVP, the base image may start as an empty virtual filesystem plus metadata.
That is acceptable as long as this limitation is explicit.

## `WORKDIR`

### Expected behavior

- Update current working directory in preview state
- Create the directory if it does not already exist
- Record provenance that it was created implicitly by `WORKDIR` if needed

## `COPY`

### Expected behavior

- Copy files from build context into virtual filesystem
- Respect destination path relative to current `WORKDIR` when appropriate
- Preserve enough source metadata to answer `:explain`

### Multi-stage note

Later versions should support `COPY --from=<stage>`.
This is essential for multi-stage value.

## `ENV`

### Expected behavior

- Update preview environment map
- Make values visible to `env`
- Record change in history/layers

## `RUN`

Handled by simulation policy.
See `05-run-simulation.md`.

## Unsupported instructions

When unsupported instructions appear:

- keep raw text in history
- mark them unsupported or ignored with a warning
- do not silently discard them

## Path handling rules

- Normalize paths deterministically
- Avoid host-specific path assumptions
- Resolve relative paths against preview cwd

## File provenance rules

For every created or modified node, record:

- which instruction created it
- whether it was copied from context or simulated by `RUN`
- whether it was later overwritten
- whether it is currently shadowed

## Acceptance examples

### Example 1

```dockerfile
FROM scratch
WORKDIR /app
COPY . /app
ENV APP_ENV=dev
```

Expected preview:

- cwd is `/app`
- `/app` exists
- context files appear under `/app`
- `APP_ENV=dev` is visible

### Example 2

```dockerfile
FROM scratch AS builder
WORKDIR /src
COPY . /src
RUN mkdir -p /out && touch /out/app
FROM scratch
COPY --from=builder /out/app /app/app
```

Expected later-stage preview:

- `/app/app` exists
- provenance mentions `COPY --from=builder`
