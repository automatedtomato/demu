---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
  - "**/Cargo.lock"
---
# Rust Coding Style

> This file extends [common/coding-style.md](../common/coding-style.md) with Rust specific content.

## Formatting

- **rustfmt** is mandatory — run `cargo fmt` before every commit, no style debates
- **clippy** warnings are treated as errors: `cargo clippy -- -D warnings`

## Error Handling

Never use `.unwrap()` or `.expect()` in library or engine code. Propagate errors with `?`:

```rust
fn load_config(path: &Path) -> Result<Config, ConfigError> {
    let contents = fs::read_to_string(path)?;
    let config = toml::from_str(&contents)?;
    Ok(config)
}
```

Use `.expect("reason")` only in:
- `main()` for fatal startup failures
- test code
- cases where the invariant is provably upheld (document why)

Define domain-specific error types with `thiserror`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unexpected token `{token}` at line {line}")]
    UnexpectedToken { token: String, line: usize },

    #[error("unterminated string literal")]
    UnterminatedString,
}
```

## Ownership & Borrowing

- Prefer borrowing (`&T`, `&mut T`) over cloning
- Clone only at system boundaries (e.g., storing user input, crossing thread boundaries)
- Use `Cow<str>` when a function may or may not need to own its string data

## Naming

Follow Rust standard conventions:
- Types, traits, enums: `UpperCamelCase`
- Functions, methods, variables, modules: `snake_case`
- Constants, statics: `SCREAMING_SNAKE_CASE`
- Lifetime parameters: short lowercase (`'a`, `'src`, `'ctx`)

## Visibility

Default to the most restrictive visibility. Expose `pub` only at intentional API boundaries:

```rust
pub struct Engine { ... }        // public API
pub(crate) fn parse(...) { ... } // crate-internal
fn helper(...) { ... }           // module-private (default)
```

## Enums over Stringly-Typed Branching

Prefer explicit enums over `&str` / `String` matching:

```rust
// Prefer this
pub enum Instruction {
    Run(String),
    Copy { src: PathBuf, dst: PathBuf },
    Env(String, String),
}

// Over this
fn handle(cmd: &str, args: &str) { match cmd { "RUN" => ..., _ => ... } }
```

## Reference

See skill: `rust-patterns` for comprehensive Rust idioms, trait design, and lifetime guidance.
