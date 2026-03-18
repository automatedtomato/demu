# Decision 003: COPY Instruction — Host File Reading

**Status:** Accepted
**Date:** 2026-03-17

## Decision

From v0.1, `COPY src dst` reads actual file content from the host filesystem and stores it in `VirtualFs`.

## Rationale

- Users can `cat` copied files immediately — this is core value of the REPL
- Without real content, `cat Dockerfile` inside demu would return nothing, which feels broken
- The user's working directory (where `demu -f Dockerfile` is run) is the natural build context root

## Behavior spec

| Scenario | Behavior |
|----------|---------|
| `src` exists on host | Read content, store as `FsNode::File` with provenance `COPY` |
| `src` is a directory | Recursively copy all files under `dst/` |
| `src` does not exist | Emit `Warning::MissingCopySource`, create empty `FsNode::File` as placeholder |
| `src` uses glob (`*.txt`) | Not supported in v0.1 — emit `Warning::UnsupportedGlob`, skip |

## Trade-offs accepted

- `COPY --from=<stage>` is not supported in v0.1 (multi-stage is v0.3 scope)
- No `.dockerignore` processing in v0.1

## Future extensibility

The `engine::copy` handler receives a `CopySource` enum. In v0.3, add `CopySource::Stage(name)` variant to handle `COPY --from=` without changing the call sites.

```rust
pub enum CopySource {
    Host(PathBuf),       // v0.1
    Stage(String),       // v0.3: COPY --from=<stage>
}
```
