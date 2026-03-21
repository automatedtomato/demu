//! Fixture-based integration tests for the Compose YAML parser.
//!
//! These tests load YAML fixture files from `tests/fixtures/compose/` and
//! assert that the parser produces the expected domain model.  Each test was
//! written **before** the implementation (TDD red phase) and the
//! implementation was evolved until all tests passed (green phase).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use demu::model::compose::{BuildConfig, EnvEntry, VolumeDefinition, VolumeSpec};
use demu::parser::compose::parse_compose;
use demu::parser::ComposeParseError;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

/// Load and parse the named fixture file, panicking with a helpful message on
/// failure.
fn load(name: &str) -> demu::model::compose::ComposeFile {
    let path = format!("tests/fixtures/compose/{name}");
    let input = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read fixture {path}: {e}"));
    parse_compose(&input).unwrap_or_else(|e| panic!("parse failed for {path}: {e}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Full fixture: two services, correct names, build config, env entries, volume
/// specs, and depends_on.
#[test]
fn test_full_compose() {
    let file = load("full.yaml");

    // Exactly two services.
    assert_eq!(file.services.len(), 2, "expected 2 services");
    assert!(file.services.contains_key("api"), "api service missing");
    assert!(file.services.contains_key("db"), "db service missing");

    // --- api service ---
    let api = file.services.get("api").expect("api");
    assert_eq!(api.name, "api");

    // Build config from long form.
    let build = api.build.as_ref().expect("api build config");
    assert_eq!(
        *build,
        BuildConfig {
            context: PathBuf::from("./services/api"),
            dockerfile: PathBuf::from("Dockerfile.dev"),
        }
    );
    assert!(api.image.is_none(), "api should have no image");

    // Environment: list form — NODE_ENV=production + bare DEBUG.
    assert!(
        api.environment.contains(&EnvEntry::KeyValue {
            key: "NODE_ENV".to_string(),
            value: "production".to_string(),
        }),
        "NODE_ENV entry missing"
    );
    assert!(
        api.environment.contains(&EnvEntry::KeyOnly {
            key: "DEBUG".to_string()
        }),
        "DEBUG entry missing"
    );

    // Volumes: bind mount (non-readonly) + bind mount (readonly).
    assert!(
        api.volumes.contains(&VolumeSpec::Bind {
            host_path: PathBuf::from("./src"),
            container_path: PathBuf::from("/app/src"),
            read_only: false,
        }),
        "non-readonly bind volume missing"
    );
    assert!(
        api.volumes.contains(&VolumeSpec::Bind {
            host_path: PathBuf::from("./src"),
            container_path: PathBuf::from("/app/src"),
            read_only: true,
        }),
        "readonly bind volume missing"
    );

    // depends_on.
    assert_eq!(api.depends_on, vec!["db"]);

    // --- db service ---
    let db = file.services.get("db").expect("db");
    assert_eq!(db.image.as_deref(), Some("postgres:15"));
    assert!(db.build.is_none(), "db should have no build config");

    // Environment from dict form.
    assert!(db.environment.contains(&EnvEntry::KeyValue {
        key: "POSTGRES_USER".to_string(),
        value: "app".to_string(),
    }));

    // Named volume.
    assert!(db.volumes.contains(&VolumeSpec::Named {
        volume_name: "pgdata".to_string(),
        container_path: PathBuf::from("/var/lib/postgresql/data"),
        read_only: false,
    }));

    // Top-level volumes declaration.
    assert!(
        file.volumes.contains_key("pgdata"),
        "top-level pgdata volume missing"
    );
}

/// Image-only service: `image` is Some, `build` is None.
#[test]
fn test_image_only_service() {
    let file = load("image_only.yaml");
    let db = file.services.get("db").expect("db service");
    assert_eq!(db.image.as_deref(), Some("postgres:15"));
    assert!(
        db.build.is_none(),
        "build should be None for image-only service"
    );
}

/// Dict form of `environment`: string value + numeric value stringified.
#[test]
fn test_env_dict_form() {
    let file = load("env_dict.yaml");
    let api = file.services.get("api").expect("api service");

    assert!(api.environment.contains(&EnvEntry::KeyValue {
        key: "NODE_ENV".to_string(),
        value: "production".to_string(),
    }));
    // Numeric value 3000 must have been stringified.
    assert!(
        api.environment.contains(&EnvEntry::KeyValue {
            key: "PORT".to_string(),
            value: "3000".to_string(),
        }),
        "PORT entry with stringified numeric value missing; got: {:?}",
        api.environment
    );
}

/// Named volume variant with correct fields.
#[test]
fn test_volume_named_fixture() {
    let file = load("volume_named.yaml");
    let db = file.services.get("db").expect("db service");

    assert!(
        db.volumes.contains(&VolumeSpec::Named {
            volume_name: "pgdata".to_string(),
            container_path: PathBuf::from("/var/lib/postgresql/data"),
            read_only: false,
        }),
        "named pgdata volume missing; got: {:?}",
        db.volumes
    );

    let vol_def = file.volumes.get("pgdata").expect("pgdata definition");
    assert_eq!(*vol_def, VolumeDefinition { external: false });
}

/// Invalid YAML returns `ComposeParseError::InvalidYaml`.
#[test]
fn test_invalid_yaml_error() {
    let path = "tests/fixtures/compose/invalid.yaml";
    let input = std::fs::read_to_string(path).expect("fixture readable");
    let err = parse_compose(&input).expect_err("should fail on invalid YAML");
    assert!(
        matches!(err, ComposeParseError::InvalidYaml { .. }),
        "expected InvalidYaml, got: {err}"
    );
}

/// YAML with no `services:` key returns `MissingServicesKey`.
#[test]
fn test_missing_services_error() {
    let yaml = "version: \"3.8\"\nvolumes:\n  data:\n";
    let err = parse_compose(yaml).expect_err("should fail without services");
    assert!(
        matches!(err, ComposeParseError::MissingServicesKey),
        "expected MissingServicesKey, got: {err}"
    );
}

/// Short-form `build: ./app` → `BuildConfig.dockerfile == "Dockerfile"`.
#[test]
fn test_build_string_shorthand_fixture() {
    let yaml = "services:\n  api:\n    build: ./app\n";
    let file = parse_compose(yaml).expect("should parse");
    let api = file.services.get("api").expect("api");
    let build = api.build.as_ref().expect("build");
    assert_eq!(build.context, PathBuf::from("./app"));
    assert_eq!(
        build.dockerfile,
        PathBuf::from("Dockerfile"),
        "default dockerfile name should be 'Dockerfile'"
    );
}

/// Unknown top-level keys (`networks:`) and service-level keys (`restart:`)
/// should not cause a parse error.
#[test]
fn test_unknown_keys_tolerated_fixture() {
    let yaml = concat!(
        "services:\n",
        "  web:\n",
        "    image: nginx\n",
        "    restart: always\n",
        "networks:\n",
        "  default:\n",
    );
    let result = parse_compose(yaml);
    assert!(
        result.is_ok(),
        "unknown keys should be tolerated, got: {result:?}"
    );
}

/// `depends_on` in dict form — only the service name keys should be collected.
#[test]
fn test_depends_on_dict_form_fixture() {
    let yaml = concat!(
        "services:\n",
        "  api:\n",
        "    image: nginx\n",
        "    depends_on:\n",
        "      db:\n",
        "        condition: service_healthy\n",
        "      cache:\n",
        "        condition: service_started\n",
        "  db:\n",
        "    image: postgres:15\n",
        "  cache:\n",
        "    image: redis:7\n",
    );
    let file = parse_compose(yaml).expect("should parse");
    let api = file.services.get("api").expect("api");
    let mut deps = api.depends_on.clone();
    deps.sort();
    assert_eq!(deps, vec!["cache", "db"]);
}
