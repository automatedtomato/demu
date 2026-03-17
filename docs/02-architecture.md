# Architecture

## System overview

The system should be split into five major layers:

1. CLI entrypoint
2. Parser layer
3. Preview engine
4. Virtual filesystem and state model
5. REPL and explanation layer

## High-level flow

```text
CLI input
  -> parse Dockerfile / Compose
  -> build internal model
  -> apply instructions into preview state
  -> open REPL over preview state
  -> answer shell and custom commands
```

## Core modules

### `cli`

Parses flags and subcommands.
Examples:

- `demu -f Dockerfile`
- `demu -f Dockerfile --stage builder`
- `demu exec -f Dockerfile -- ls /app`
- `demu --compose -f compose.yaml --service api`

### `parser`

Responsible for turning files into typed models.

Subareas:

- Dockerfile parser
- Compose parser
- `.env` reading later if needed

### `model`

Contains the typed domain model.
Suggested submodels:

- Dockerfile AST / instruction model
- Compose service model
- Virtual filesystem nodes
- Preview state
- Package registry
- Provenance metadata

### `engine`

Applies parsed input into preview state.

Subareas:

- Dockerfile instruction interpreter
- `RUN` simulator
- stage resolver
- Compose merger

### `repl`

Provides shell-like interaction.

Standard commands:

- `ls`
- `cd`
- `pwd`
- `cat`
- `find`
- `env`
- `exit`

Custom commands:

- `:layers`
- `:history`
- `:installed`
- `:explain <path>`
- `:mounts`
- `:services`
- `:stage`

### `explain`

Answers provenance questions.
Examples:

- which instruction created this file?
- was it overwritten?
- did it come from `COPY --from=builder`?
- is it shadowed by a mount?

## Key data structures

### Preview state

The preview state should capture everything visible to the user.

Suggested shape:

```rust
struct PreviewState {
    cwd: PathBuf,
    env: BTreeMap<String, String>,
    fs: VirtualFs,
    installed: InstalledRegistry,
    history: Vec<HistoryEntry>,
    layers: Vec<LayerSummary>,
    warnings: Vec<Warning>,
    active_stage: Option<String>,
}
```

### Virtual filesystem node

```rust
enum FsNode {
    File(FileNode),
    Directory(DirNode),
    Symlink(SymlinkNode),
}
```

### Provenance

Every node should carry enough metadata to answer `:explain`.

```rust
struct Provenance {
    created_by: ProvenanceSource,
    modified_by: Vec<ProvenanceSource>,
    shadowed_by_mount: Option<MountInfo>,
}
```

## Design requirement: provenance is first-class

Do not bolt provenance on later.
It is central to the value of `demu`.

## Determinism

Given the same inputs, the preview should be stable and deterministic.
Tests should not depend on wall-clock time or machine-specific state.
