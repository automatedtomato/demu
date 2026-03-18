# Decision 001: VirtualFs Internal Representation

**Status:** Accepted
**Date:** 2026-03-17

## Decision

Use a flat `HashMap<PathBuf, FsNode>` as the internal representation of `VirtualFs`.

```rust
pub struct VirtualFs {
    nodes: HashMap<PathBuf, FsNode>,
}
```

## Rationale

- `COPY src dst` maps directly to `insert(dst, node)` — no tree traversal needed
- `ls <dir>` is implemented by filtering keys with `starts_with(dir)` and depth check
- Deterministic iteration when sorted by key
- Simple to serialize/snapshot for tests

## Trade-offs accepted

- `ls -R` and `find` require a linear scan instead of a recursive walk — acceptable at v0.1 scale
- No inode-level hard link modeling — out of scope

## Future extensibility

If a tree structure becomes necessary (e.g., for efficient directory renames or large image layers), migrate to a `BTreeMap<PathBuf, FsNode>` first (free sorted iteration), then consider a proper arena-based tree only if profiling shows a bottleneck.

See also: `tasks/decisions/` for related decisions.
