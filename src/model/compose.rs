//! Domain model types for Docker Compose files.
//!
//! These types represent the clean, validated view of a `docker-compose.yml`
//! after parsing.  They intentionally do **not** derive `Deserialize` — that
//! concern lives in the parser module where intermediate raw types are used
//! to handle Compose's many dual-form fields (string *or* map, list *or* dict,
//! etc.).
//!
//! # Module layout
//!
//! | Type | Purpose |
//! |------|---------|
//! | `ComposeFile` | Top-level container: services + named-volume declarations |
//! | `Service` | One `docker-compose.yml` service definition |
//! | `BuildConfig` | Build context and Dockerfile path |
//! | `EnvEntry` | A single environment variable (key=value or bare key) |
//! | `VolumeSpec` | A per-service volume mount (bind, named, or anonymous) |
//! | `VolumeDefinition` | A top-level named volume declaration |

use std::collections::BTreeMap;
use std::path::PathBuf;

/// The top-level structure of a parsed `docker-compose.yml`.
///
/// `services` is always present (parsing fails if the key is missing).
/// `volumes` may be empty when no top-level named volumes are declared.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposeFile {
    /// All service definitions keyed by service name.
    pub services: BTreeMap<String, Service>,
    /// Top-level named volume declarations keyed by volume name.
    pub volumes: BTreeMap<String, VolumeDefinition>,
}

/// A single service definition inside a Compose file.
///
/// Fields mirror the most commonly used Compose keys.  Unknown keys in the
/// source YAML are silently ignored — this matches Docker's own tolerant
/// parsing behaviour and keeps the preview useful even for complex files.
#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    /// The service name (taken from the YAML map key, not a field inside the
    /// service map).
    pub name: String,
    /// Build configuration when the service is built from a local context.
    pub build: Option<BuildConfig>,
    /// Pre-built image reference (e.g. `"postgres:15"`).
    pub image: Option<String>,
    /// Environment variables injected into the container.
    pub environment: Vec<EnvEntry>,
    /// Paths to env-file(s) whose contents are injected as environment
    /// variables.
    ///
    /// # Security note
    /// Stored verbatim from the YAML — may contain `..` components. Callers
    /// must validate these paths against the project root before reading them
    /// from the host filesystem.
    pub env_file: Vec<PathBuf>,
    /// Volume mounts for this service.
    pub volumes: Vec<VolumeSpec>,
    /// Working directory override inside the container.
    pub working_dir: Option<PathBuf>,
    /// Names of services that must start before this one.
    pub depends_on: Vec<String>,
    /// Port mappings (kept as raw strings — format varies widely and we only
    /// need them for informational display, not actual binding).
    pub ports: Vec<String>,
}

/// Build configuration for a service that is built from local source.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildConfig {
    /// The build context directory (e.g. `"."` or `"./services/api"`).
    ///
    /// # Security note
    /// Stored verbatim from the YAML — may contain `..` components. Callers
    /// that use this path for host filesystem access **must** canonicalize it
    /// and verify it does not escape the declared project root before opening
    /// any file.
    pub context: PathBuf,
    /// Path to the Dockerfile relative to `context`.  Defaults to
    /// `"Dockerfile"` when only a short-form string is given.
    ///
    /// # Security note
    /// Stored verbatim from the YAML. Same path-containment requirement as
    /// `context` applies.
    pub dockerfile: PathBuf,
}

/// A single environment variable entry.
///
/// Compose supports two forms:
/// - `KEY=value` or dict `{KEY: value}` → [`EnvEntry::KeyValue`]
/// - bare `KEY` (value taken from the host environment at runtime) →
///   [`EnvEntry::KeyOnly`]
#[derive(Debug, Clone, PartialEq)]
pub enum EnvEntry {
    /// A fully-specified `KEY=value` pair.
    KeyValue { key: String, value: String },
    /// A key with no value — the runtime inherits it from the host environment.
    KeyOnly { key: String },
}

/// A volume mount specification for a service.
///
/// Compose supports three variants:
/// - **Bind mount** — host path mapped into the container
/// - **Named volume** — a Docker-managed named volume
/// - **Anonymous** — no source, just a container path
#[derive(Debug, Clone, PartialEq)]
pub enum VolumeSpec {
    /// A bind mount from a host path into the container.
    ///
    /// # Security note
    /// `host_path` is stored verbatim from the YAML and may contain `..`
    /// components. It is used only as display/shadow metadata in the preview
    /// — it must **never** be opened on the host filesystem without first
    /// canonicalizing and confirming it does not escape the project root.
    Bind {
        host_path: PathBuf,
        container_path: PathBuf,
        read_only: bool,
    },
    /// A named volume (managed by Docker).
    Named {
        volume_name: String,
        container_path: PathBuf,
        read_only: bool,
    },
    /// An anonymous volume with only a container path.
    Anonymous { container_path: PathBuf },
}

/// A top-level named volume declaration.
///
/// Currently only tracks whether the volume is declared as `external` (i.e.
/// it must already exist and Compose should not attempt to create it).
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeDefinition {
    /// `true` when `external: true` is set in the volume declaration.
    pub external: bool,
}
