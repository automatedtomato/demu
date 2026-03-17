# Test strategy

## Testing philosophy

Test visible behavior, not just internal implementation.

The key question is:

> does the preview world shown to the user match the intended approximation?

## Test layers

### Unit tests

For:

- path normalization
- filesystem operations
- package registry behavior
- parser edge cases
- provenance updates

### Integration tests

For:

- Dockerfile fixture -> preview state
- REPL commands over preview state
- `RUN` simulation scenarios
- multi-stage scenarios
- Compose service merge scenarios later

## Fixture strategy

Create small, focused fixture directories.

Suggested layout:

```text
tests/
├── fixtures/
│   ├── basic_copy/
│   │   ├── Dockerfile
│   │   └── src/
│   ├── workdir_env/
│   ├── run_fs_mutation/
│   ├── run_install_registry/
│   ├── multi_stage_copy/
│   └── compose_api_db/
└── integration/
```

Each fixture should be easy to reason about in isolation.

## Acceptance tests for MVP

### Fixture: basic copy

Given:

- `WORKDIR /app`
- `COPY . /app`

Expect:

- cwd is `/app`
- copied files appear under `/app`

### Fixture: env

Given:

- `ENV APP_ENV=dev`

Expect:

- `env` output includes `APP_ENV=dev`

### Fixture: run fs mutation

Given:

- `RUN mkdir -p /app/logs && touch /app/logs/app.log`

Expect:

- directory and file exist in preview filesystem

### Fixture: run install registry

Given:

- `RUN apt-get install -y curl git`
- `RUN pip install fastapi`

Expect:

- `:installed apt` contains `curl` and `git`
- `:installed pip` contains `fastapi`
- `apt list --installed` and `pip list` show simulated entries

### Fixture: explain

Given:

- file created by `COPY`

Expect:

- `:explain <path>` points to that instruction

## Stability requirements

- tests must not depend on Docker daemon
- tests must not depend on network
- tests must not depend on host shell tools
- tests should be deterministic across machines

## Snapshot testing

Okay for:

- `:layers`
- `:history`
- `:explain`
- pseudo command outputs

Use carefully and keep fixtures small.
