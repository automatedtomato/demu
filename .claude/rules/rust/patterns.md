---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
  - "**/Cargo.lock"
---
# Rust Patterns

> This file extends [common/patterns.md](../common/patterns.md) with Rust specific content.

## Builder Pattern

Use the builder pattern for structs with many optional fields:

```rust
#[derive(Default)]
pub struct EngineBuilder {
    workdir: Option<PathBuf>,
    env: HashMap<String, String>,
    strict: bool,
}

impl EngineBuilder {
    pub fn workdir(mut self, path: impl Into<PathBuf>) -> Self {
        self.workdir = Some(path.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.env.insert(key.into(), val.into());
        self
    }

    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    pub fn build(self) -> Engine {
        Engine { workdir: self.workdir.unwrap_or_default(), env: self.env, strict: self.strict }
    }
}
```

## Newtype Pattern

Wrap primitive types to enforce domain invariants at the type level:

```rust
pub struct LayerId(u32);
pub struct StageName(String);

impl StageName {
    pub fn new(s: impl Into<String>) -> Result<Self, ValidationError> {
        let s = s.into();
        if s.is_empty() { return Err(ValidationError::EmptyName); }
        Ok(Self(s))
    }
}
```

## Typestate Pattern

Use phantom types to encode state transitions at compile time:

```rust
pub struct Stage<S> { inner: StageInner, _state: PhantomData<S> }

pub struct Building;
pub struct Sealed;

impl Stage<Building> {
    pub fn add_layer(mut self, layer: Layer) -> Self { ... }
    pub fn seal(self) -> Stage<Sealed> { ... }
}

impl Stage<Sealed> {
    pub fn filesystem(&self) -> &Filesystem { ... }
}
```

## Trait-Based Abstraction

Prefer generics over `dyn Trait` for zero-cost abstraction; use `dyn Trait` only when runtime polymorphism is necessary:

```rust
// Zero-cost: monomorphized at compile time
fn simulate<R: RunHandler>(handler: &R, cmd: &str) -> Result<()> { ... }

// Runtime dispatch: use when storing mixed types in a collection
let handlers: Vec<Box<dyn RunHandler>> = vec![...];
```

## `impl Trait` in Return Position

Use `impl Trait` to hide implementation details in return types:

```rust
pub fn supported_commands() -> impl Iterator<Item = &'static str> {
    SUPPORTED.iter().copied()
}
```

## Extension Traits

Add domain-specific methods to foreign types via extension traits:

```rust
pub trait PathExt {
    fn relative_to(&self, base: &Path) -> Option<&Path>;
}

impl PathExt for Path {
    fn relative_to(&self, base: &Path) -> Option<&Path> {
        self.strip_prefix(base).ok()
    }
}
```

## Reference

See skill: `rust-patterns` for comprehensive Rust patterns including concurrency, state machines, and trait design.
