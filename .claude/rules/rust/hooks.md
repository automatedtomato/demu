---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
  - "**/Cargo.lock"
---
# Rust Hooks

> This file extends [common/hooks.md](../common/hooks.md) with Rust specific content.

## PostToolUse Hooks

Configure in `~/.claude/settings.json`:

- **cargo fmt**: Auto-format `.rs` files after every edit
- **cargo clippy**: Run linter after editing `.rs` files — treat warnings as errors
- **cargo check**: Fast type-check after editing `.rs` or `Cargo.toml` files (faster than a full build)

### Recommended hook configuration

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": { "tool_name": "Edit", "file_paths": ["**/*.rs"] },
        "hooks": [
          { "type": "command", "command": "cargo fmt --" },
          { "type": "command", "command": "cargo clippy -- -D warnings" }
        ]
      },
      {
        "matcher": { "tool_name": "Edit", "file_paths": ["**/Cargo.toml"] },
        "hooks": [
          { "type": "command", "command": "cargo check" }
        ]
      }
    ]
  }
}
```

## Notes

- `cargo fmt --` formats only the file being edited when passed a path; omit `--` to format the whole workspace
- `cargo check` is significantly faster than `cargo build` and sufficient for catching type errors during editing
- For large workspaces, scope clippy to the edited crate: `cargo clippy -p <crate-name> -- -D warnings`
