// `:services` command — list all services in the Compose file.
//
// In Compose mode, shows a table of all services with their type (build vs
// image) and their `depends_on` list. The currently selected service is
// prefixed with `*`.
//
// In single-Dockerfile mode (no ComposeContext), prints a clear usage hint
// so the user knows how to invoke Compose mode.
//
// Example output (Compose mode with 3 services, "api" selected):
//
//   Services (3 total):
//   * api     build: Dockerfile   depends: db, redis
//     db      image: postgres:15
//     redis   image: redis:7
//
// Column widths are derived from the longest name to keep columns aligned.

use std::io::Write;

use crate::output::sanitize::sanitize_for_terminal;
use crate::repl::config::ComposeContext;
use crate::repl::error::ReplError;

/// Guard message printed when `:services` is invoked outside Compose mode.
const GUARD_MSG: &str = "\
:services is only available in compose mode
  Usage: demu --compose -f compose.yaml --service <name>
";

/// Execute the `:services` command.
///
/// When `ctx` is `None` the guard message is printed.
/// When `ctx` is `Some`, a formatted service table is printed.
pub fn execute(ctx: Option<&ComposeContext>, writer: &mut impl Write) -> Result<(), ReplError> {
    let io_err = |e: std::io::Error| ReplError::InvalidArguments {
        command: ":services".to_string(),
        message: e.to_string(),
    };

    let ctx = match ctx {
        Some(c) => c,
        None => {
            write!(writer, "{GUARD_MSG}").map_err(io_err)?;
            return Ok(());
        }
    };

    let services = &ctx.compose_file.services;
    let selected = &ctx.selected_service;

    // Determine column width from the longest service name (min 8 chars).
    let name_width = services.keys().map(|k| k.len()).max().unwrap_or(0).max(8);

    writeln!(writer, "Services ({} total):", services.len()).map_err(io_err)?;

    // BTreeMap iterates in sorted order, which keeps output deterministic.
    for (name, svc) in services {
        let prefix = if name == selected { '*' } else { ' ' };

        // Build type description: "build: <dockerfile>" or "image: <image>".
        let type_str = match (&svc.build, &svc.image) {
            (Some(build), _) => {
                let df = sanitize_for_terminal(&build.dockerfile.display().to_string());
                format!("build: {df}")
            }
            (None, Some(image)) => {
                let safe = sanitize_for_terminal(image);
                format!("image: {safe}")
            }
            (None, None) => "image: <unknown>".to_string(),
        };

        // Build depends_on string: "depends: a, b" or empty.
        let depends_str = if svc.depends_on.is_empty() {
            String::new()
        } else {
            let names: Vec<String> = svc
                .depends_on
                .iter()
                .map(|d| sanitize_for_terminal(d))
                .collect();
            format!("  depends: {}", names.join(", "))
        };

        let safe_name = sanitize_for_terminal(name);
        writeln!(
            writer,
            "{prefix} {safe_name:<name_width$}  {type_str}{depends_str}"
        )
        .map_err(io_err)?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::model::compose::{BuildConfig, ComposeFile, Service, VolumeDefinition};
    use crate::repl::config::ComposeContext;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn run(ctx: Option<&ComposeContext>) -> String {
        let mut buf = Vec::new();
        execute(ctx, &mut buf).expect("services should not fail");
        String::from_utf8(buf).expect("utf-8")
    }

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

    fn compose_with_services(services: Vec<Service>) -> ComposeFile {
        let mut map = BTreeMap::new();
        for svc in services {
            map.insert(svc.name.clone(), svc);
        }
        ComposeFile {
            services: map,
            volumes: BTreeMap::new(),
        }
    }

    // --- guard mode ---

    #[test]
    fn no_context_prints_guard_message() {
        let out = run(None);
        assert!(
            out.contains(":services is only available in compose mode"),
            "guard message missing; got: {out}"
        );
        assert!(
            out.contains("--compose"),
            "usage hint must mention --compose; got: {out}"
        );
    }

    // --- service type: image ---

    #[test]
    fn image_only_service_shows_image_label() {
        let svc = Service {
            image: Some("postgres:15".to_string()),
            ..bare_service("db")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![svc]),
            selected_service: "db".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(out.contains("image: postgres:15"), "got: {out}");
    }

    // --- service type: build ---

    #[test]
    fn build_service_shows_dockerfile_name() {
        let svc = Service {
            build: Some(BuildConfig {
                context: PathBuf::from("."),
                dockerfile: PathBuf::from("Dockerfile"),
            }),
            ..bare_service("api")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![svc]),
            selected_service: "api".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(out.contains("build: Dockerfile"), "got: {out}");
    }

    // --- selected service is marked with * ---

    #[test]
    fn selected_service_is_marked_with_asterisk() {
        let api = Service {
            image: Some("nginx".to_string()),
            ..bare_service("api")
        };
        let db = Service {
            image: Some("postgres:15".to_string()),
            ..bare_service("db")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![api, db]),
            selected_service: "api".to_string(),
        };
        let out = run(Some(&ctx));
        // "* api" must appear in the output.
        assert!(
            out.contains("* api"),
            "selected service must have * prefix; got:\n{out}"
        );
        // "  db" (with space prefix) must appear.
        assert!(
            out.contains("  db"),
            "non-selected service must have space prefix; got:\n{out}"
        );
    }

    // --- depends_on is listed ---

    #[test]
    fn depends_on_appears_in_output() {
        let api = Service {
            image: Some("myapp".to_string()),
            depends_on: vec!["db".to_string(), "redis".to_string()],
            ..bare_service("api")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![api]),
            selected_service: "api".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(out.contains("depends: db, redis"), "got: {out}");
    }

    // --- service with no depends_on shows no depends label ---

    #[test]
    fn no_depends_on_shows_no_depends_label() {
        let db = Service {
            image: Some("postgres:15".to_string()),
            ..bare_service("db")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![db]),
            selected_service: "db".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(
            !out.contains("depends:"),
            "no depends_on must not show label; got:\n{out}"
        );
    }

    // --- header shows total count ---

    #[test]
    fn header_shows_service_count() {
        let api = Service {
            image: Some("nginx".to_string()),
            ..bare_service("api")
        };
        let db = Service {
            image: Some("postgres:15".to_string()),
            ..bare_service("db")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![api, db]),
            selected_service: "api".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(
            out.contains("Services (2 total):"),
            "header missing; got: {out}"
        );
    }

    // --- ANSI sanitization ---

    #[test]
    fn ansi_escape_in_service_name_is_stripped() {
        let evil = Service {
            image: Some("postgres".to_string()),
            ..bare_service("db\x1b[2J")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![evil]),
            selected_service: "db\x1b[2J".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(
            !out.contains('\x1b'),
            "ANSI escapes must be stripped; got: {out:?}"
        );
    }
}
