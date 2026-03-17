---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
  - "**/Cargo.lock"
---
# Rust Testing

> This file extends [common/testing.md](../common/testing.md) with Rust specific content.

## Framework

Use the built-in `#[test]` attribute with `cargo test`. No external test framework needed for unit tests.

## Test Organization

- **Unit tests**: inline in the same file, inside a `#[cfg(test)]` module
- **Integration tests**: under `tests/` directory, one file per feature area
- **Doc tests**: in `///` doc comments for public API examples

```rust
// src/parser/mod.rs
pub fn parse_instruction(line: &str) -> Result<Instruction, ParseError> { ... }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_run_instruction() {
        let inst = parse_instruction("RUN apt-get install -y curl").unwrap();
        assert_eq!(inst, Instruction::Run("apt-get install -y curl".into()));
    }

    #[test]
    fn rejects_empty_line() {
        assert!(parse_instruction("").is_err());
    }
}
```

## Integration Test Fixtures

Store fixture Dockerfiles and expected outputs under `tests/fixtures/`:

```
tests/
├── fixtures/
│   ├── basic_copy/
│   │   ├── Dockerfile
│   │   └── expected.json
│   └── multi_stage/
│       ├── Dockerfile
│       └── expected.json
└── integration_test.rs
```

## Assertions

- Prefer `assert_eq!` / `assert_ne!` over `assert!(a == b)` — better failure messages
- Use `assert_matches!` (nightly) or pattern matching for enum variants:

```rust
assert!(matches!(result, Err(ParseError::UnexpectedToken { .. })));
```

## Running Tests

```bash
# All tests
cargo test

# With output shown
cargo test -- --nocapture

# Specific test by name
cargo test parses_run_instruction

# Integration tests only
cargo test --test integration_test
```

## Coverage

```bash
cargo llvm-cov --html
```

Or with `cargo-tarpaulin` as an alternative:

```bash
cargo tarpaulin --out Html
```

## Reference

See skill: `rust-testing` for detailed Rust testing patterns, fixture helpers, and snapshot testing.
