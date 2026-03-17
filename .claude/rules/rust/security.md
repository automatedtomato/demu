---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
  - "**/Cargo.lock"
---
# Rust Security

> This file extends [common/security.md](../common/security.md) with Rust specific content.

## `unsafe` Policy

- `unsafe` blocks require a `// SAFETY:` comment explaining the invariant being upheld
- No `unsafe` in parser, engine, or REPL modules — safe Rust is sufficient
- Every `unsafe` block must be reviewed separately in code review

```rust
// SAFETY: `ptr` is non-null and points to a valid `T` allocated by this module.
let value = unsafe { &*ptr };
```

## No Panics in Library Code

- Ban `.unwrap()` and `.expect()` without justification in `src/` (excluding `main.rs` and tests)
- Use clippy lint to enforce: add to `Cargo.toml`:

```toml
[lints.clippy]
unwrap_used = "warn"
expect_used = "warn"
```

## Secret Management

Read secrets from environment variables, never hardcode:

```rust
let api_key = std::env::var("API_KEY")
    .map_err(|_| ConfigError::MissingEnvVar("API_KEY"))?;
```

## Dependency Auditing

Run `cargo audit` regularly to check for known vulnerabilities in dependencies:

```bash
cargo install cargo-audit
cargo audit
```

Pin dependencies in `Cargo.lock` (check it into version control for binaries).

## Input Validation

Validate all user-supplied input at the boundary (CLI args, file contents) before passing into the engine:

```rust
pub fn parse_dockerfile(input: &str) -> Result<Vec<Instruction>, ParseError> {
    // Validate and reject malformed input here, not deeper in the engine
}
```

## Integer Overflow

In debug builds Rust panics on overflow. In release builds it wraps silently. Use checked arithmetic when processing untrusted sizes:

```rust
let total = count.checked_mul(size).ok_or(ArithmeticError::Overflow)?;
```
