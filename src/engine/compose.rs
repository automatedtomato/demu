//! Compose service engine — selects a service and builds its `PreviewState`.
//!
//! The single public entry point is [`run_compose`], which:
//!
//! 1. Selects the named service from a parsed [`ComposeFile`].
//! 2. Resolves and runs the service's Dockerfile through the existing
//!    [`engine::run`] pipeline (or starts with an empty state for image-only
//!    services).
//! 3. Applies Compose-level overrides: `env_file`, `environment`, `working_dir`.
//! 4. Returns a [`ComposeEngineOutput`] with the merged state.
//!
//! # Override precedence (highest → lowest)
//! 1. `environment` entries in the Compose service definition
//! 2. `env_file` entries (last file wins for duplicate keys)
//! 3. `ENV` instructions in the Dockerfile
//!
//! # Security notes
//! - `compose_dir` is canonicalized once at the start of `run_compose`. If it
//!   cannot be resolved the function returns `ComposeEngineError::Io`.
//! - `build.context` and `build.dockerfile` paths are canonicalized and checked
//!   against `canonical_compose_dir` to prevent reading arbitrary host files via
//!   path traversal (`../../../etc/passwd`). Paths that escape the root return
//!   [`ComposeEngineError::DockerfileNotFound`].
//! - `env_file` paths are validated with the same canonicalize-and-contains-check.
//!   Paths that escape the project root emit [`Warning::EnvFileNotFound`] and are
//!   skipped.
//! - Service names and file paths embedded in error messages are sanitized
//!   by the caller (`main.rs`) before terminal output.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::engine;
use crate::model::compose::{ComposeFile, EnvEntry};
use crate::model::fs::{DirNode, FsNode};
use crate::model::provenance::{Provenance, ProvenanceSource};
use crate::model::state::PreviewState;
use crate::model::warning::Warning;
use crate::parser::dockerfile::parse_dockerfile;

use super::copy::ensure_ancestors;
use super::mount::apply_mount_shadows;

/// Errors that prevent the Compose engine from producing a `PreviewState`.
///
/// These are unrecoverable failures. Recoverable conditions (missing env_file,
/// unresolved env key) become [`Warning`]s in the returned state instead.
#[derive(Debug, Error)]
pub enum ComposeEngineError {
    /// The requested service name was not found in the Compose file.
    #[error("service '{name}' not found in compose file")]
    ServiceNotFound { name: String },

    /// The Dockerfile referenced by the service's build config does not exist.
    #[error("Dockerfile not found: '{}'", path.display())]
    DockerfileNotFound { path: PathBuf },

    /// The Dockerfile content could not be parsed.
    #[error("Dockerfile parse error: {message}")]
    ParseError { message: String },

    /// The engine failed while processing the Dockerfile instructions.
    #[error("engine error: {message}")]
    EngineError { message: String },

    /// A host filesystem I/O error occurred that was not recoverable.
    #[error("I/O error: {message}")]
    Io { message: String },
}

/// The output of a successful compose engine run.
#[derive(Debug)]
pub struct ComposeEngineOutput {
    /// The merged `PreviewState` reflecting Dockerfile + Compose overrides.
    pub state: PreviewState,
    /// The original `ComposeFile` (owned copy, used by `:reload`).
    pub compose_file: ComposeFile,
    /// The name of the service that was selected.
    pub selected_service: String,
}

/// Run the Compose service pipeline.
///
/// # Arguments
/// - `compose_file` — the parsed Compose file
/// - `service_name` — the service to preview (must exist in `compose_file`)
/// - `compose_dir` — the directory containing the `compose.yaml` file;
///   used to resolve relative `build.context`, `env_file`, and `working_dir`
///   paths
///
/// # Returns
/// A [`ComposeEngineOutput`] with the merged preview state on success, or a
/// [`ComposeEngineError`] for unrecoverable failures. Recoverable conditions
/// (missing env_file, unresolved env keys, image-only service) are recorded as
/// warnings in the returned state.
pub fn run_compose(
    compose_file: &ComposeFile,
    service_name: &str,
    compose_dir: &Path,
) -> Result<ComposeEngineOutput, ComposeEngineError> {
    // ── 0. Canonicalize compose_dir once — required for all path-containment checks.
    //
    // Security: we fail hard here rather than using a fallback. If compose_dir
    // cannot be resolved, all downstream containment checks would be unreliable.
    let canonical_compose_dir = compose_dir
        .canonicalize()
        .map_err(|e| ComposeEngineError::Io {
            message: format!(
                "cannot canonicalize compose directory '{}': {e}",
                compose_dir.display()
            ),
        })?;

    // ── 1. Select the service ─────────────────────────────────────────────────

    let service = compose_file.services.get(service_name).ok_or_else(|| {
        ComposeEngineError::ServiceNotFound {
            name: service_name.to_string(),
        }
    })?;

    // ── 2. Build initial state (Dockerfile or empty) ──────────────────────────

    let mut state = match &service.build {
        Some(build_config) => {
            // Security: canonicalize both the context directory and the Dockerfile
            // path, and verify each stays within canonical_compose_dir.
            // This prevents path-traversal attacks via `context: ../../..` or
            // `dockerfile: ../../.ssh/id_rsa` in attacker-controlled YAML.

            let resolved_context = compose_dir.join(&build_config.context);
            let canonical_context = resolved_context.canonicalize().map_err(|_| {
                ComposeEngineError::DockerfileNotFound {
                    path: resolved_context.clone(),
                }
            })?;
            if !canonical_context.starts_with(&canonical_compose_dir) {
                return Err(ComposeEngineError::DockerfileNotFound {
                    path: canonical_context,
                });
            }

            let resolved_dockerfile = canonical_context.join(&build_config.dockerfile);
            let canonical_dockerfile = resolved_dockerfile.canonicalize().map_err(|_| {
                ComposeEngineError::DockerfileNotFound {
                    path: resolved_dockerfile.clone(),
                }
            })?;
            if !canonical_dockerfile.starts_with(&canonical_compose_dir) {
                return Err(ComposeEngineError::DockerfileNotFound {
                    path: canonical_dockerfile,
                });
            }

            let content = std::fs::read_to_string(&canonical_dockerfile).map_err(|e| {
                ComposeEngineError::Io {
                    message: format!("cannot read '{}': {e}", canonical_dockerfile.display()),
                }
            })?;

            let instructions =
                parse_dockerfile(&content).map_err(|e| ComposeEngineError::ParseError {
                    message: e.to_string(),
                })?;

            let engine_output = engine::run(instructions, &canonical_context).map_err(|e| {
                ComposeEngineError::EngineError {
                    message: e.to_string(),
                }
            })?;

            engine_output.state
        }
        None => {
            // Image-only service: start from an empty state and record a warning.
            let mut s = PreviewState::default();
            let image = service
                .image
                .clone()
                .unwrap_or_else(|| "<unknown>".to_string());
            s.active_stage = Some(image.clone());
            s.warnings.push(Warning::ImageOnlyService { image });
            s
        }
    };

    // ── 3. Apply env_file entries (lower precedence than environment) ─────────

    for env_file_path in &service.env_file {
        let resolved = compose_dir.join(env_file_path);

        // Security: canonicalize and check the path stays within compose_dir.
        // Paths that escape the project root are treated as missing.
        let safe = match resolved.canonicalize() {
            Ok(canon) => {
                if canon.starts_with(&canonical_compose_dir) {
                    Some(canon)
                } else {
                    // Path escaped the project root — treat as missing.
                    None
                }
            }
            Err(_) => None,
        };

        match safe {
            Some(safe_path) => match std::fs::read_to_string(&safe_path) {
                Ok(content) => parse_env_file(&content, &mut state.env),
                Err(_) => {
                    state.warnings.push(Warning::EnvFileNotFound {
                        path: env_file_path.clone(),
                    });
                }
            },
            None => {
                state.warnings.push(Warning::EnvFileNotFound {
                    path: env_file_path.clone(),
                });
            }
        }
    }

    // ── 4. Apply environment entries (highest precedence) ─────────────────────

    for entry in &service.environment {
        match entry {
            EnvEntry::KeyValue { key, value } => {
                state.env.insert(key.clone(), value.clone());
            }
            EnvEntry::KeyOnly { key } => {
                // At preview time the host environment is not available.
                // Record a warning and skip rather than inserting a wrong value.
                state
                    .warnings
                    .push(Warning::UnresolvedEnvKey { key: key.clone() });
            }
        }
    }

    // ── 5. Apply working_dir override ─────────────────────────────────────────

    if let Some(ref working_dir) = service.working_dir {
        let raw_cwd = if working_dir.is_absolute() {
            working_dir.clone()
        } else {
            state.cwd.join(working_dir)
        };

        // Normalize the virtual path by resolving `..` components. If `..`
        // would traverse above `/`, clamp to `/` and emit a warning so the
        // user knows the CWD was adjusted.
        let new_cwd = normalize_virtual_path(&raw_cwd);
        if new_cwd != raw_cwd {
            state.warnings.push(Warning::WorkdirEscapedRoot {
                path: working_dir.clone(),
            });
        }

        // Ensure the directory and its ancestors exist in the virtual filesystem,
        // mirroring how the engine handles WORKDIR instructions.
        let prov = ProvenanceSource::Workdir;
        ensure_ancestors(&mut state.fs, &new_cwd, prov.clone());
        if !state.fs.contains(&new_cwd) {
            state.fs.insert(
                new_cwd.clone(),
                FsNode::Directory(DirNode {
                    provenance: Provenance::new(prov),
                    permissions: None,
                }),
            );
        }

        state.cwd = new_cwd;
    }

    // ── 6. Apply volume mount shadows ─────────────────────────────────────────

    apply_mount_shadows(&mut state, &service.volumes);

    Ok(ComposeEngineOutput {
        state,
        compose_file: compose_file.clone(),
        selected_service: service_name.to_string(),
    })
}

/// Normalize a virtual (non-host) path by resolving `..` components.
///
/// Unlike `Path::canonicalize`, this operates on the virtual filesystem and
/// never touches the host. If `..` would traverse above the root `/`, that
/// component is discarded and the path is clamped at `/`.
///
/// # Examples
/// ```text
/// /app/../etc  →  /etc          (normal resolution)
/// /app/../../  →  /             (clamped: cannot go above /)
/// /a/b/c       →  /a/b/c        (no change)
/// ```
fn normalize_virtual_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut components: Vec<&std::ffi::OsStr> = Vec::new();

    for component in path.components() {
        match component {
            Component::RootDir => {
                components.clear();
                // Push nothing — we rebuild the root at join time.
            }
            Component::Normal(seg) => {
                components.push(seg);
            }
            Component::ParentDir => {
                // Pop the last segment; if empty, we're at root — ignore.
                components.pop();
            }
            // CurDir (.) and Prefix are no-ops in this context.
            Component::CurDir | Component::Prefix(_) => {}
        }
    }

    let mut result = PathBuf::from("/");
    for seg in components {
        result.push(seg);
    }
    result
}

/// Parse a `.env` file and insert entries into `env`.
///
/// Format rules (following Docker Compose env_file conventions):
/// - Lines starting with `#` are comments and are skipped.
/// - Empty or whitespace-only lines are skipped.
/// - Lines of the form `KEY=value` are inserted; `value` may be empty.
/// - Lines without `=` are skipped (bare `KEY` form is not valid in env_file).
/// - Inline comments (`KEY=value # comment`) are stripped from values.
/// - A single matching pair of surrounding `"` or `'` quotes is removed from
///   values (`KEY="hello"` → `hello`, `KEY='world'` → `world`).
fn parse_env_file(content: &str, env: &mut std::collections::BTreeMap<String, String>) {
    for line in content.lines() {
        let line = line.trim();
        // Skip comments and blank lines.
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Split on the first `=` only; everything after is the raw value.
        if let Some(eq_pos) = line.find('=') {
            let key = &line[..eq_pos];
            let raw_value = &line[eq_pos + 1..];
            if key.is_empty() {
                continue;
            }

            // Strip inline comment: `value # comment` → `value`.
            // Only strip `#` that is preceded by unquoted whitespace so that
            // values like `URL=http://host/#anchor` are not truncated.
            let value_no_comment = strip_inline_comment(raw_value);

            // Strip a single matching pair of surrounding quotes.
            let value = strip_surrounding_quotes(value_no_comment);

            env.insert(key.to_string(), value.to_string());
        }
        // Lines without `=` are skipped (bare KEY form is not valid in env_file).
    }
}

/// Strip an inline comment from an env file value.
///
/// A `#` is treated as the start of a comment only when it is preceded by
/// at least one ASCII whitespace character and is not inside a quoted section.
/// This handles the common case while preserving URLs with `#` fragments
/// when the value is quoted.
fn strip_inline_comment(value: &str) -> &str {
    // If the value is quoted, do not strip inline comments — the whole quoted
    // string is the value. The quote stripping happens afterward.
    let trimmed = value.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
    {
        return value;
    }

    // Find the first ` #` (space followed by hash) and truncate there.
    if let Some(pos) = value.find(" #") {
        return value[..pos].trim_end();
    }
    value.trim_end()
}

/// Strip a single matching pair of surrounding quotes from a value.
///
/// `"hello"` → `hello`, `'world'` → `world`.
/// Unmatched or nested quotes are left as-is.
fn strip_surrounding_quotes(value: &str) -> &str {
    let v = value.trim();
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
    {
        return &v[1..v.len() - 1];
    }
    v
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::compose::{
        BuildConfig, ComposeFile, EnvEntry, Service, VolumeDefinition, VolumeSpec,
    };
    use std::collections::BTreeMap;
    use std::io::Write;
    use tempfile::TempDir;

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Build a minimal ComposeFile with a single service.
    fn single_service_compose(service: Service) -> ComposeFile {
        let mut services = BTreeMap::new();
        services.insert(service.name.clone(), service);
        ComposeFile {
            services,
            volumes: BTreeMap::new(),
        }
    }

    /// Build a bare Service with no fields set.
    fn bare_service(name: &str) -> Service {
        Service {
            name: name.to_string(),
            build: None,
            image: None,
            environment: vec![],
            env_file: vec![],
            volumes: vec![],
            working_dir: None,
            depends_on: vec![],
            ports: vec![],
        }
    }

    /// Create a temp directory with a Dockerfile and return (TempDir, ComposeFile).
    fn make_build_fixture(dockerfile_content: &str) -> (TempDir, ComposeFile) {
        let dir = TempDir::new().expect("tempdir");
        let df_path = dir.path().join("Dockerfile");
        std::fs::write(&df_path, dockerfile_content).expect("write Dockerfile");

        let service = Service {
            build: Some(BuildConfig {
                context: PathBuf::from("."),
                dockerfile: PathBuf::from("Dockerfile"),
            }),
            ..bare_service("svc")
        };
        (dir, single_service_compose(service))
    }

    // ── test 1: service not found ─────────────────────────────────────────────

    #[test]
    fn service_not_found_returns_error() {
        let compose = ComposeFile {
            services: BTreeMap::new(),
            volumes: BTreeMap::new(),
        };
        let err = run_compose(&compose, "missing", Path::new("/tmp")).expect_err("should fail");
        assert!(
            matches!(err, ComposeEngineError::ServiceNotFound { .. }),
            "expected ServiceNotFound, got: {err:?}"
        );
    }

    // ── test 2: image-only service ────────────────────────────────────────────

    #[test]
    fn image_only_service_returns_empty_fs_with_warning() {
        let service = Service {
            image: Some("postgres:15".to_string()),
            ..bare_service("db")
        };
        let compose = single_service_compose(service);

        let output = run_compose(&compose, "db", Path::new("/tmp")).expect("should succeed");

        // Filesystem should be effectively empty (no files from Dockerfile).
        assert!(
            output.state.fs.iter().count() == 0,
            "image-only service must have empty fs; got {} nodes",
            output.state.fs.iter().count()
        );

        // Warning::ImageOnlyService must be present.
        let has_warning =
            output.state.warnings.iter().any(
                |w| matches!(w, Warning::ImageOnlyService { image } if image == "postgres:15"),
            );
        assert!(has_warning, "must have ImageOnlyService warning");

        // active_stage should be set to the image name.
        assert_eq!(
            output.state.active_stage.as_deref(),
            Some("postgres:15"),
            "active_stage must be the image name"
        );
    }

    // ── test 3: build service produces dockerfile filesystem ──────────────────

    #[test]
    fn build_service_produces_dockerfile_fs() {
        let (dir, compose) = make_build_fixture("FROM scratch\nWORKDIR /app\nENV DF_VAR=hello\n");

        let output = run_compose(&compose, "svc", dir.path()).expect("should succeed");

        // /app should exist in the virtual filesystem from WORKDIR.
        let app_path = PathBuf::from("/app");
        assert!(
            output.state.fs.contains(&app_path),
            "virtual fs must contain /app from WORKDIR"
        );

        // DF_VAR should be set from Dockerfile ENV.
        assert_eq!(
            output.state.env.get("DF_VAR").map(String::as_str),
            Some("hello"),
            "DF_VAR must be set from Dockerfile ENV"
        );
    }

    // ── test 4: compose environment wins over dockerfile ENV ──────────────────

    #[test]
    fn env_merge_compose_wins_over_dockerfile() {
        let (dir, mut compose) = make_build_fixture("FROM scratch\nENV SHARED=from_dockerfile\n");

        // Add Compose environment override.
        compose.services.get_mut("svc").unwrap().environment = vec![EnvEntry::KeyValue {
            key: "SHARED".to_string(),
            value: "from_compose".to_string(),
        }];

        let output = run_compose(&compose, "svc", dir.path()).expect("should succeed");

        assert_eq!(
            output.state.env.get("SHARED").map(String::as_str),
            Some("from_compose"),
            "Compose environment must override Dockerfile ENV"
        );
    }

    // ── test 5: working_dir override ─────────────────────────────────────────

    #[test]
    fn working_dir_override_sets_cwd() {
        let (dir, mut compose) = make_build_fixture("FROM scratch\nWORKDIR /app\n");

        compose.services.get_mut("svc").unwrap().working_dir = Some(PathBuf::from("/override"));

        let output = run_compose(&compose, "svc", dir.path()).expect("should succeed");

        assert_eq!(
            output.state.cwd,
            PathBuf::from("/override"),
            "working_dir must override Dockerfile WORKDIR"
        );

        // /override should also exist in the virtual filesystem.
        assert!(
            output.state.fs.contains(&PathBuf::from("/override")),
            "virtual fs must contain /override after working_dir override"
        );
    }

    // ── test 6: env_file is loaded and merged ─────────────────────────────────

    #[test]
    fn env_file_loaded_and_merged() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("Dockerfile"), b"FROM scratch\n").expect("write df");
        let mut env_file = std::fs::File::create(dir.path().join(".env.test")).expect("create");
        writeln!(env_file, "ENV_FILE_VAR=from_file").expect("write env");

        let service = Service {
            build: Some(BuildConfig {
                context: PathBuf::from("."),
                dockerfile: PathBuf::from("Dockerfile"),
            }),
            env_file: vec![PathBuf::from(".env.test")],
            ..bare_service("svc")
        };
        let compose = single_service_compose(service);

        let output = run_compose(&compose, "svc", dir.path()).expect("should succeed");

        assert_eq!(
            output.state.env.get("ENV_FILE_VAR").map(String::as_str),
            Some("from_file"),
            "env var from env_file must be in state.env"
        );
    }

    // ── test 7: missing env_file emits warning without crashing ───────────────

    #[test]
    fn missing_env_file_emits_warning() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("Dockerfile"), b"FROM scratch\n").expect("write df");

        let service = Service {
            build: Some(BuildConfig {
                context: PathBuf::from("."),
                dockerfile: PathBuf::from("Dockerfile"),
            }),
            env_file: vec![PathBuf::from(".env.nonexistent")],
            ..bare_service("svc")
        };
        let compose = single_service_compose(service);

        let output = run_compose(&compose, "svc", dir.path()).expect("must not error");

        let has_warning = output
            .state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::EnvFileNotFound { .. }));
        assert!(has_warning, "must have EnvFileNotFound warning");
    }

    // ── test 8: KeyOnly env emits UnresolvedEnvKey warning ────────────────────

    #[test]
    fn key_only_env_emits_unresolved_warning() {
        let (dir, mut compose) = make_build_fixture("FROM scratch\n");

        compose.services.get_mut("svc").unwrap().environment = vec![EnvEntry::KeyOnly {
            key: "HOST_VAR".to_string(),
        }];

        let output = run_compose(&compose, "svc", dir.path()).expect("should succeed");

        let has_warning = output
            .state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnresolvedEnvKey { key } if key == "HOST_VAR"));
        assert!(
            has_warning,
            "must have UnresolvedEnvKey warning for HOST_VAR"
        );

        assert!(
            !output.state.env.contains_key("HOST_VAR"),
            "HOST_VAR must NOT be inserted into env"
        );
    }

    // ── test 9: env_file before environment precedence ────────────────────────

    #[test]
    fn env_file_before_environment_precedence() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("Dockerfile"), b"FROM scratch\n").expect("write df");
        let mut ef = std::fs::File::create(dir.path().join(".env")).expect("create");
        writeln!(ef, "KEY=from_file").expect("write");

        let service = Service {
            build: Some(BuildConfig {
                context: PathBuf::from("."),
                dockerfile: PathBuf::from("Dockerfile"),
            }),
            env_file: vec![PathBuf::from(".env")],
            environment: vec![EnvEntry::KeyValue {
                key: "KEY".to_string(),
                value: "from_compose".to_string(),
            }],
            ..bare_service("svc")
        };
        let compose = single_service_compose(service);

        let output = run_compose(&compose, "svc", dir.path()).expect("should succeed");

        // environment entries have higher precedence than env_file entries.
        assert_eq!(
            output.state.env.get("KEY").map(String::as_str),
            Some("from_compose"),
            "environment entries must win over env_file entries"
        );
    }

    // ── test 10: dockerfile not found returns error ───────────────────────────

    #[test]
    fn dockerfile_not_found_returns_error() {
        let dir = TempDir::new().expect("tempdir");
        // Do NOT create Dockerfile — it should be missing.

        let service = Service {
            build: Some(BuildConfig {
                context: PathBuf::from("."),
                dockerfile: PathBuf::from("Dockerfile"),
            }),
            ..bare_service("svc")
        };
        let compose = single_service_compose(service);

        let err = run_compose(&compose, "svc", dir.path()).expect_err("should fail");
        assert!(
            matches!(err, ComposeEngineError::DockerfileNotFound { .. }),
            "expected DockerfileNotFound, got: {err:?}"
        );
    }

    // ── test 11: parse_env_file parses standard format ────────────────────────

    #[test]
    fn parse_env_file_handles_comments_and_blanks() {
        let content = "# comment\n\nKEY=value\nANOTHER=one\n";
        let mut env = BTreeMap::new();
        parse_env_file(content, &mut env);
        assert_eq!(env.get("KEY").map(String::as_str), Some("value"));
        assert_eq!(env.get("ANOTHER").map(String::as_str), Some("one"));
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn parse_env_file_handles_empty_value() {
        let content = "EMPTY=\n";
        let mut env = BTreeMap::new();
        parse_env_file(content, &mut env);
        assert_eq!(env.get("EMPTY").map(String::as_str), Some(""));
    }

    #[test]
    fn parse_env_file_skips_lines_without_equals() {
        let content = "BARE_KEY\nKEY=value\n";
        let mut env = BTreeMap::new();
        parse_env_file(content, &mut env);
        assert!(!env.contains_key("BARE_KEY"), "bare key should be skipped");
        assert_eq!(env.get("KEY").map(String::as_str), Some("value"));
    }

    #[test]
    fn parse_env_file_value_may_contain_equals() {
        let content = "URL=https://example.com/?a=1&b=2\n";
        let mut env = BTreeMap::new();
        parse_env_file(content, &mut env);
        assert_eq!(
            env.get("URL").map(String::as_str),
            Some("https://example.com/?a=1&b=2")
        );
    }

    // ── test 12: output fields are populated correctly ────────────────────────

    #[test]
    fn output_fields_are_populated() {
        let (dir, compose) = make_build_fixture("FROM scratch\n");

        let output = run_compose(&compose, "svc", dir.path()).expect("should succeed");

        assert_eq!(output.selected_service, "svc");
        assert!(output.compose_file.services.contains_key("svc"));
    }

    // ── test 13: volume mounts are applied as shadows ─────────────────────────

    #[test]
    fn bind_volume_recorded_in_state_mounts() {
        let (dir, mut compose) = make_build_fixture("FROM scratch\nWORKDIR /app\n");

        compose.services.get_mut("svc").unwrap().volumes = vec![VolumeSpec::Bind {
            host_path: PathBuf::from("./data"),
            container_path: PathBuf::from("/data"),
            read_only: false,
        }];

        let output = run_compose(&compose, "svc", dir.path()).expect("should succeed");

        assert_eq!(
            output.state.mounts.len(),
            1,
            "one bind mount must be recorded"
        );
        assert!(
            output.state.mounts[0]
                .description
                .contains("bind mount from"),
            "description must mention bind mount; got: {}",
            output.state.mounts[0].description
        );
    }

    #[test]
    fn named_volume_recorded_in_state_mounts() {
        let (dir, mut compose) = make_build_fixture("FROM scratch\n");

        compose.services.get_mut("svc").unwrap().volumes = vec![VolumeSpec::Named {
            volume_name: "my-cache".to_string(),
            container_path: PathBuf::from("/cache"),
            read_only: false,
        }];

        let output = run_compose(&compose, "svc", dir.path()).expect("should succeed");

        assert_eq!(output.state.mounts.len(), 1);
        assert_eq!(output.state.mounts[0].description, "named volume: my-cache");
    }

    // ── test 14: #60 — path traversal via build.context is rejected ───────────

    #[test]
    fn context_path_traversal_returns_dockerfile_not_found() {
        let dir = TempDir::new().expect("tempdir");

        // Try to escape the compose directory via `..` in the context path.
        let service = Service {
            build: Some(BuildConfig {
                context: PathBuf::from("../.."),
                dockerfile: PathBuf::from("Dockerfile"),
            }),
            ..bare_service("svc")
        };
        let compose = single_service_compose(service);

        let err = run_compose(&compose, "svc", dir.path()).expect_err("should fail");
        assert!(
            matches!(err, ComposeEngineError::DockerfileNotFound { .. }),
            "path traversal via context must return DockerfileNotFound; got: {err:?}"
        );
    }

    #[test]
    fn dockerfile_path_traversal_returns_dockerfile_not_found() {
        let dir = TempDir::new().expect("tempdir");
        // Create a file outside the temp dir to verify the check triggers before reading.
        // We just need the traversal attempt to be rejected regardless of whether
        // the target exists.
        let service = Service {
            build: Some(BuildConfig {
                context: PathBuf::from("."),
                dockerfile: PathBuf::from("../../some_file"),
            }),
            ..bare_service("svc")
        };
        let compose = single_service_compose(service);

        let err = run_compose(&compose, "svc", dir.path()).expect_err("should fail");
        assert!(
            matches!(err, ComposeEngineError::DockerfileNotFound { .. }),
            "dockerfile path traversal must return DockerfileNotFound; got: {err:?}"
        );
    }

    // ── test 15: #61 — working_dir with .. components is normalized ───────────

    #[test]
    fn working_dir_with_dotdot_is_normalized() {
        let (dir, mut compose) = make_build_fixture("FROM scratch\nWORKDIR /app\n");

        // Relative working_dir with `..` that would escape / — should be clamped.
        compose.services.get_mut("svc").unwrap().working_dir = Some(PathBuf::from("../../../etc"));

        let output = run_compose(&compose, "svc", dir.path()).expect("should succeed");

        // The CWD must not contain `..` components.
        let cwd_str = output.state.cwd.to_string_lossy();
        assert!(
            !cwd_str.contains(".."),
            "normalized CWD must not contain '..'; got: {cwd_str}"
        );

        // A WorkdirEscapedRoot warning must be emitted.
        let has_warning = output
            .state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::WorkdirEscapedRoot { .. }));
        assert!(has_warning, "must have WorkdirEscapedRoot warning");
    }

    #[test]
    fn working_dir_absolute_with_dotdot_is_normalized() {
        let (dir, mut compose) = make_build_fixture("FROM scratch\n");

        compose.services.get_mut("svc").unwrap().working_dir =
            Some(PathBuf::from("/app/../../etc"));

        let output = run_compose(&compose, "svc", dir.path()).expect("should succeed");

        assert_eq!(
            output.state.cwd,
            PathBuf::from("/etc"),
            "absolute path with .. should normalize correctly"
        );
        // No warning needed — path stays within virtual FS (doesn't escape root).
    }

    // ── test 16: normalize_virtual_path unit tests ────────────────────────────

    #[test]
    fn normalize_virtual_path_resolves_dotdot() {
        assert_eq!(
            normalize_virtual_path(&PathBuf::from("/app/../etc")),
            PathBuf::from("/etc")
        );
    }

    #[test]
    fn normalize_virtual_path_clamps_above_root() {
        assert_eq!(
            normalize_virtual_path(&PathBuf::from("/../../etc")),
            PathBuf::from("/etc")
        );
    }

    #[test]
    fn normalize_virtual_path_no_change() {
        assert_eq!(
            normalize_virtual_path(&PathBuf::from("/app/data")),
            PathBuf::from("/app/data")
        );
    }

    #[test]
    fn normalize_virtual_path_root() {
        assert_eq!(
            normalize_virtual_path(&PathBuf::from("/")),
            PathBuf::from("/")
        );
    }

    // ── test 17: #62 — parse_env_file strips inline comments and quotes ───────

    #[test]
    fn parse_env_file_strips_inline_comment() {
        let content = "KEY=value # this is a comment\n";
        let mut env = BTreeMap::new();
        parse_env_file(content, &mut env);
        assert_eq!(
            env.get("KEY").map(String::as_str),
            Some("value"),
            "inline comment must be stripped; got: {:?}",
            env.get("KEY")
        );
    }

    #[test]
    fn parse_env_file_strips_double_quotes() {
        let content = "KEY=\"hello world\"\n";
        let mut env = BTreeMap::new();
        parse_env_file(content, &mut env);
        assert_eq!(
            env.get("KEY").map(String::as_str),
            Some("hello world"),
            "double quotes must be stripped; got: {:?}",
            env.get("KEY")
        );
    }

    #[test]
    fn parse_env_file_strips_single_quotes() {
        let content = "KEY='single quoted'\n";
        let mut env = BTreeMap::new();
        parse_env_file(content, &mut env);
        assert_eq!(
            env.get("KEY").map(String::as_str),
            Some("single quoted"),
            "single quotes must be stripped; got: {:?}",
            env.get("KEY")
        );
    }

    #[test]
    fn parse_env_file_url_with_hash_in_quotes_preserved() {
        // A quoted URL with a `#` fragment should NOT have the fragment stripped.
        let content = "URL=\"http://host/#anchor\"\n";
        let mut env = BTreeMap::new();
        parse_env_file(content, &mut env);
        assert_eq!(
            env.get("URL").map(String::as_str),
            Some("http://host/#anchor"),
            "hash in quoted value must be preserved; got: {:?}",
            env.get("URL")
        );
    }

    #[test]
    fn parse_env_file_unquoted_url_with_equals_preserved() {
        // Value with embedded `=` must not be truncated.
        let content = "URL=https://example.com/?a=1\n";
        let mut env = BTreeMap::new();
        parse_env_file(content, &mut env);
        assert_eq!(
            env.get("URL").map(String::as_str),
            Some("https://example.com/?a=1")
        );
    }
}
