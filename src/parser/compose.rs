//! Parser for `docker-compose.yml` files.
//!
//! # Design
//!
//! Compose YAML uses "dual-form" fields extensively: `build` can be a plain
//! string or a map; `environment` can be a list or a dict; `depends_on` can
//! be a list or a map; etc.  Serde handles this through `#[serde(untagged)]`
//! enums defined *privately* in this module.  The public surface only exposes
//! the clean domain types from [`crate::model::compose`].
//!
//! # Entry point
//!
//! [`parse_compose`] is the single public function.  It deserialises the raw
//! YAML into private intermediate types, validates required keys, then
//! converts everything into the domain model.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::model::compose::{
    BuildConfig, ComposeFile, EnvEntry, Service, VolumeDefinition, VolumeSpec,
};
use crate::parser::error::ComposeParseError;

// ---------------------------------------------------------------------------
// Private serde intermediate types
// ---------------------------------------------------------------------------

/// Top-level Compose file structure.
///
/// `services` is `Option` so we can detect its absence and return
/// [`ComposeParseError::MissingServicesKey`] rather than a confusing YAML
/// error.  Unknown top-level keys (e.g. `networks:`, `version:`) are ignored
/// thanks to `#[serde(default)]` and the lack of `deny_unknown_fields`.
#[derive(Deserialize, Default)]
struct RawComposeFile {
    services: Option<BTreeMap<String, RawService>>,
    #[serde(default)]
    volumes: BTreeMap<String, Option<RawVolumeDefinition>>,
}

/// Raw service definition — all fields are optional so we never fail on an
/// unknown or absent key.
#[derive(Deserialize, Default)]
struct RawService {
    build: Option<RawBuild>,
    image: Option<String>,
    #[serde(default)]
    environment: Option<RawEnv>,
    #[serde(default)]
    env_file: Option<RawEnvFile>,
    #[serde(default)]
    volumes: Vec<RawVolumeSpec>,
    working_dir: Option<String>,
    #[serde(default)]
    depends_on: Option<RawDependsOn>,
    #[serde(default)]
    ports: Vec<String>,
}

/// `build` can be a plain string (short form) or a map (long form).
#[derive(Deserialize)]
#[serde(untagged)]
enum RawBuild {
    /// Short form: `build: ./app`
    Short(String),
    /// Long form: `build: { context: ./app, dockerfile: Dockerfile.dev }`
    Full {
        context: String,
        #[serde(default = "default_dockerfile")]
        dockerfile: String,
    },
}

fn default_dockerfile() -> String {
    "Dockerfile".to_string()
}

/// `environment` can be a list of strings or a mapping.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawEnv {
    /// List form: `- KEY=value` or `- KEY`
    List(Vec<String>),
    /// Dict form: `KEY: value` (values may be strings, numbers, bools)
    Dict(BTreeMap<String, serde_yaml::Value>),
}

/// `env_file` can be a single string or a list.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawEnvFile {
    Single(String),
    List(Vec<String>),
}

/// `depends_on` can be a list of service names or a map with condition objects.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawDependsOn {
    List(Vec<String>),
    /// Dict form: `depends_on: { db: { condition: service_healthy } }`
    /// We only collect the keys (service names); conditions are not modelled.
    Dict(BTreeMap<String, serde_yaml::Value>),
}

/// A single volume spec in the per-service `volumes:` list.
///
/// Each entry is either a short string (`"./src:/app:ro"`) or a long map form.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawVolumeSpec {
    Short(String),
    Long {
        #[serde(rename = "type")]
        kind: String,
        source: Option<String>,
        target: String,
        #[serde(default)]
        read_only: bool,
    },
}

/// A top-level named volume declaration.  May be `null` (bare name) or a map.
#[derive(Deserialize, Default)]
struct RawVolumeDefinition {
    #[serde(default)]
    external: bool,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Parse a `docker-compose.yml` file from a string slice.
///
/// Returns a [`ComposeFile`] on success, or a [`ComposeParseError`] describing
/// the first structural problem encountered.
///
/// Unknown YAML keys (e.g. `networks:`, `restart:`, `healthcheck:`) are
/// silently ignored so that real-world Compose files remain usable even when
/// they use features that `demu` does not yet model.
pub fn parse_compose(input: &str) -> Result<ComposeFile, ComposeParseError> {
    // Deserialise raw YAML into the intermediate representation.
    let raw: RawComposeFile =
        serde_yaml::from_str(input).map_err(|e| ComposeParseError::InvalidYaml {
            message: e.to_string(),
        })?;

    // `services:` is required — fail with a clear message if absent.
    let raw_services = raw.services.ok_or(ComposeParseError::MissingServicesKey)?;

    // Convert each raw service into the domain model.
    let mut services = BTreeMap::new();
    for (name, raw_svc) in raw_services {
        let service = convert_service(name.clone(), raw_svc)?;
        services.insert(name, service);
    }

    // Convert top-level volume declarations.
    let volumes = raw
        .volumes
        .into_iter()
        .map(|(name, raw_vol)| {
            let def = VolumeDefinition {
                external: raw_vol.map(|v| v.external).unwrap_or(false),
            };
            (name, def)
        })
        .collect();

    Ok(ComposeFile { services, volumes })
}

// ---------------------------------------------------------------------------
// Private conversion helpers
// ---------------------------------------------------------------------------

/// Convert a [`RawService`] into a [`Service`] domain type.
fn convert_service(name: String, raw: RawService) -> Result<Service, ComposeParseError> {
    let build = raw.build.map(convert_build);
    let environment = convert_env(raw.environment);
    let env_file = convert_env_file(raw.env_file);
    let volumes = raw
        .volumes
        .into_iter()
        .map(|v| convert_volume_spec(v, &name))
        .collect::<Result<Vec<_>, _>>()?;
    let depends_on = convert_depends_on(raw.depends_on);

    Ok(Service {
        name,
        build,
        image: raw.image,
        environment,
        env_file,
        volumes,
        working_dir: raw.working_dir.map(PathBuf::from),
        depends_on,
        ports: raw.ports,
    })
}

/// Convert a [`RawBuild`] into a [`BuildConfig`].
fn convert_build(raw: RawBuild) -> BuildConfig {
    match raw {
        RawBuild::Short(ctx) => BuildConfig {
            context: PathBuf::from(&ctx),
            dockerfile: PathBuf::from("Dockerfile"),
        },
        RawBuild::Full {
            context,
            dockerfile,
        } => BuildConfig {
            context: PathBuf::from(context),
            dockerfile: PathBuf::from(dockerfile),
        },
    }
}

/// Convert a [`RawEnv`] (or absent value) into a list of [`EnvEntry`] items.
fn convert_env(raw: Option<RawEnv>) -> Vec<EnvEntry> {
    match raw {
        None => vec![],
        Some(RawEnv::List(items)) => items.into_iter().map(parse_env_string).collect(),
        Some(RawEnv::Dict(map)) => map
            .into_iter()
            .map(|(k, v)| EnvEntry::KeyValue {
                key: k,
                value: yaml_value_to_string(v),
            })
            .collect(),
    }
}

/// Parse a single environment string in `KEY` or `KEY=value` form.
fn parse_env_string(s: String) -> EnvEntry {
    // Split on the *first* `=` only so that values containing `=` are
    // preserved intact (e.g. `BASE64=abc=def`).
    match s.split_once('=') {
        Some((key, value)) => EnvEntry::KeyValue {
            key: key.to_string(),
            value: value.to_string(),
        },
        None => EnvEntry::KeyOnly { key: s },
    }
}

/// Convert a `serde_yaml::Value` to a string representation.
///
/// This handles the common case of numeric and boolean values appearing in
/// the `environment` dict form (e.g. `PORT: 3000`).
fn yaml_value_to_string(v: serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s,
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Null => String::new(),
        // Nested sequences/mappings are stringified as YAML — unusual but safe.
        other => serde_yaml::to_string(&other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

/// Convert a [`RawEnvFile`] (or absent value) into a list of [`PathBuf`]s.
fn convert_env_file(raw: Option<RawEnvFile>) -> Vec<PathBuf> {
    match raw {
        None => vec![],
        Some(RawEnvFile::Single(s)) => vec![PathBuf::from(s)],
        Some(RawEnvFile::List(items)) => items.into_iter().map(PathBuf::from).collect(),
    }
}

/// Convert a [`RawDependsOn`] (or absent value) into a list of service name
/// strings.
fn convert_depends_on(raw: Option<RawDependsOn>) -> Vec<String> {
    match raw {
        None => vec![],
        Some(RawDependsOn::List(items)) => items,
        Some(RawDependsOn::Dict(map)) => map.into_keys().collect(),
    }
}

/// Convert a [`RawVolumeSpec`] into a [`VolumeSpec`] domain type.
///
/// Returns an error only when the long form specifies an unrecognised `type`
/// value — in practice this should not happen with valid Compose files.
fn convert_volume_spec(
    raw: RawVolumeSpec,
    service_name: &str,
) -> Result<VolumeSpec, ComposeParseError> {
    match raw {
        RawVolumeSpec::Short(s) => Ok(parse_volume_short(&s)),
        RawVolumeSpec::Long {
            kind,
            source,
            target,
            read_only,
        } => {
            match kind.as_str() {
                "bind" => {
                    // Long-form bind requires a source.
                    let host_path = source.map(PathBuf::from).ok_or_else(|| {
                        ComposeParseError::InvalidService {
                            name: service_name.to_string(),
                            message: "bind volume missing 'source'".to_string(),
                        }
                    })?;
                    Ok(VolumeSpec::Bind {
                        host_path,
                        container_path: PathBuf::from(target),
                        read_only,
                    })
                }
                "volume" => {
                    let volume_name = source.unwrap_or_default();
                    if volume_name.is_empty() {
                        Ok(VolumeSpec::Anonymous {
                            container_path: PathBuf::from(target),
                        })
                    } else {
                        Ok(VolumeSpec::Named {
                            volume_name,
                            container_path: PathBuf::from(target),
                            read_only,
                        })
                    }
                }
                // Unknown long-form type — treat as anonymous with the target
                // path so the file is still usable.
                _ => Ok(VolumeSpec::Anonymous {
                    container_path: PathBuf::from(target),
                }),
            }
        }
    }
}

/// Parse the short volume syntax `[source:]target[:options]`.
///
/// Classification rules:
/// - No source → `Anonymous`
/// - Source starts with `.` or `/` → `Bind` (host path)
/// - Otherwise → `Named` (Docker-managed named volume)
fn parse_volume_short(s: &str) -> VolumeSpec {
    // Options are always the last colon-separated token when present.
    // We need to be careful: an absolute path like `/host/path:/app` has three
    // parts but no options; `./src:/app:ro` has three parts with options.
    let parts: Vec<&str> = s.splitn(3, ':').collect();

    match parts.as_slice() {
        // Single segment → anonymous volume.
        [target] => VolumeSpec::Anonymous {
            container_path: PathBuf::from(*target),
        },
        // Two segments → source:target, no options.
        [source, target] => classify_source(source, target, false),
        // Three segments → source:target:options.
        [source, target, options] => {
            let read_only = *options == "ro";
            classify_source(source, target, read_only)
        }
        // Unreachable with splitn(3) but satisfies exhaustiveness.
        _ => VolumeSpec::Anonymous {
            container_path: PathBuf::from(s),
        },
    }
}

/// Classify a volume source string as a bind mount or a named volume.
fn classify_source(source: &str, target: &str, read_only: bool) -> VolumeSpec {
    if source.starts_with('.') || source.starts_with('/') {
        VolumeSpec::Bind {
            host_path: PathBuf::from(source),
            container_path: PathBuf::from(target),
            read_only,
        }
    } else {
        VolumeSpec::Named {
            volume_name: source.to_string(),
            container_path: PathBuf::from(target),
            read_only,
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Volume short-syntax tests
    // ------------------------------------------------------------------

    #[test]
    fn parse_volume_short_bind() {
        // Relative source path → Bind mount, not read-only.
        let spec = parse_volume_short("./src:/app/src");
        assert_eq!(
            spec,
            VolumeSpec::Bind {
                host_path: PathBuf::from("./src"),
                container_path: PathBuf::from("/app/src"),
                read_only: false,
            }
        );
    }

    #[test]
    fn parse_volume_short_bind_readonly() {
        // `:ro` suffix → read_only true.
        let spec = parse_volume_short("./src:/app/src:ro");
        assert_eq!(
            spec,
            VolumeSpec::Bind {
                host_path: PathBuf::from("./src"),
                container_path: PathBuf::from("/app/src"),
                read_only: true,
            }
        );
    }

    #[test]
    fn parse_volume_short_named() {
        // Non-path source → Named volume.
        let spec = parse_volume_short("data:/var/lib/data");
        assert_eq!(
            spec,
            VolumeSpec::Named {
                volume_name: "data".to_string(),
                container_path: PathBuf::from("/var/lib/data"),
                read_only: false,
            }
        );
    }

    #[test]
    fn parse_volume_short_anonymous() {
        // Single path with no source → Anonymous.
        let spec = parse_volume_short("/app/cache");
        assert_eq!(
            spec,
            VolumeSpec::Anonymous {
                container_path: PathBuf::from("/app/cache"),
            }
        );
    }

    #[test]
    fn parse_volume_short_absolute_bind() {
        // Absolute host path → Bind mount.
        let spec = parse_volume_short("/host/path:/app");
        assert_eq!(
            spec,
            VolumeSpec::Bind {
                host_path: PathBuf::from("/host/path"),
                container_path: PathBuf::from("/app"),
                read_only: false,
            }
        );
    }

    // ------------------------------------------------------------------
    // Environment entry parsing tests
    // ------------------------------------------------------------------

    #[test]
    fn parse_env_list_key_value() {
        let entry = parse_env_string("KEY=value".to_string());
        assert_eq!(
            entry,
            EnvEntry::KeyValue {
                key: "KEY".to_string(),
                value: "value".to_string()
            }
        );
    }

    #[test]
    fn parse_env_list_key_only() {
        let entry = parse_env_string("KEY".to_string());
        assert_eq!(
            entry,
            EnvEntry::KeyOnly {
                key: "KEY".to_string()
            }
        );
    }

    #[test]
    fn parse_env_dict_string_value() {
        // Dict form with a plain string value.
        let yaml = "services:\n  svc:\n    environment:\n      KEY: val\n";
        let file = parse_compose(yaml).expect("should parse");
        let svc = file.services.get("svc").expect("svc present");
        assert!(svc.environment.contains(&EnvEntry::KeyValue {
            key: "KEY".to_string(),
            value: "val".to_string(),
        }));
    }

    #[test]
    fn parse_env_dict_numeric_value() {
        // Numeric values in the dict form must be stringified.
        let yaml = "services:\n  svc:\n    environment:\n      PORT: 3000\n";
        let file = parse_compose(yaml).expect("should parse");
        let svc = file.services.get("svc").expect("svc present");
        assert!(svc.environment.contains(&EnvEntry::KeyValue {
            key: "PORT".to_string(),
            value: "3000".to_string(),
        }));
    }

    // ------------------------------------------------------------------
    // Build config tests
    // ------------------------------------------------------------------

    #[test]
    fn build_string_shorthand() {
        // Short-form string → dockerfile defaults to "Dockerfile".
        let yaml = "services:\n  api:\n    build: ./app\n";
        let file = parse_compose(yaml).expect("should parse");
        let svc = file.services.get("api").expect("api present");
        let build = svc.build.as_ref().expect("build present");
        assert_eq!(build.context, PathBuf::from("./app"));
        assert_eq!(build.dockerfile, PathBuf::from("Dockerfile"));
    }

    #[test]
    fn build_map_with_dockerfile() {
        // Long-form map → explicit dockerfile path preserved.
        let yaml = "services:\n  api:\n    build:\n      context: ./app\n      dockerfile: Dockerfile.dev\n";
        let file = parse_compose(yaml).expect("should parse");
        let svc = file.services.get("api").expect("api present");
        let build = svc.build.as_ref().expect("build present");
        assert_eq!(build.context, PathBuf::from("./app"));
        assert_eq!(build.dockerfile, PathBuf::from("Dockerfile.dev"));
    }

    // ------------------------------------------------------------------
    // depends_on tests
    // ------------------------------------------------------------------

    #[test]
    fn depends_on_list() {
        let yaml =
            "services:\n  api:\n    image: nginx\n    depends_on:\n      - db\n      - cache\n";
        let file = parse_compose(yaml).expect("should parse");
        let svc = file.services.get("api").expect("api present");
        let mut deps = svc.depends_on.clone();
        deps.sort();
        assert_eq!(deps, vec!["cache", "db"]);
    }

    #[test]
    fn depends_on_dict() {
        // Dict form — only service names (keys) should be collected.
        let yaml =
            "services:\n  api:\n    image: nginx\n    depends_on:\n      db:\n        condition: service_healthy\n";
        let file = parse_compose(yaml).expect("should parse");
        let svc = file.services.get("api").expect("api present");
        assert_eq!(svc.depends_on, vec!["db"]);
    }

    // ------------------------------------------------------------------
    // env_file tests
    // ------------------------------------------------------------------

    #[test]
    fn env_file_string() {
        let yaml = "services:\n  svc:\n    image: nginx\n    env_file: .env\n";
        let file = parse_compose(yaml).expect("should parse");
        let svc = file.services.get("svc").expect("svc present");
        assert_eq!(svc.env_file, vec![PathBuf::from(".env")]);
    }

    #[test]
    fn env_file_list() {
        let yaml =
            "services:\n  svc:\n    image: nginx\n    env_file:\n      - .env\n      - .env.local\n";
        let file = parse_compose(yaml).expect("should parse");
        let svc = file.services.get("svc").expect("svc present");
        assert_eq!(
            svc.env_file,
            vec![PathBuf::from(".env"), PathBuf::from(".env.local")]
        );
    }

    // ------------------------------------------------------------------
    // Error case tests
    // ------------------------------------------------------------------

    #[test]
    fn missing_services_key() {
        // Valid YAML but no `services:` → MissingServicesKey.
        let yaml = "version: \"3.8\"\nnetworks:\n  default:\n";
        let err = parse_compose(yaml).expect_err("should fail");
        assert!(
            matches!(err, ComposeParseError::MissingServicesKey),
            "unexpected error variant: {err}"
        );
    }

    #[test]
    fn invalid_yaml() {
        // Syntactically broken YAML → InvalidYaml.
        let yaml = "services:\n  api:\n    build: [unclosed\n";
        let err = parse_compose(yaml).expect_err("should fail");
        assert!(
            matches!(err, ComposeParseError::InvalidYaml { .. }),
            "unexpected error variant: {err}"
        );
    }

    #[test]
    fn unknown_keys_tolerated() {
        // Extra keys like `networks:` and `restart:` must not cause errors.
        let yaml =
            "services:\n  svc:\n    image: nginx\n    restart: always\nnetworks:\n  default:\n";
        let result = parse_compose(yaml);
        assert!(result.is_ok(), "should tolerate unknown keys: {result:?}");
    }
}
