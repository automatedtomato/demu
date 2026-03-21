// `:depends` command — render the dependency tree for the selected service.
//
// In Compose mode, performs a depth-first traversal of `depends_on` starting
// from the selected service and prints an indented tree:
//
//   api
//     └─ db
//     └─ redis
//
// # Cycle detection
// Cycles are detected via an `on_stack` visited set. When a node that is
// already on the current DFS path is visited again, traversal stops at that
// edge and a `[cycle detected]` marker is appended inline.
//
// # Diamond / shared dependency deduplication
// A second `fully_visited` set tracks nodes whose subtrees have been fully
// expanded in any branch. When a node is in `fully_visited`, the tree prints
// `[shared, see above]` instead of re-expanding the subtree. This prevents
// duplicate output for diamond dependency patterns.
//
// # Validation
// If `ctx.selected_service` is not found in the service map, a warning line
// is printed and the command returns without a tree — avoiding output that
// looks like a valid (but wrong) leaf-service view.
//
// In single-Dockerfile mode (no ComposeContext), prints a clear usage hint.

use std::collections::HashSet;
use std::io::Write;

use crate::output::sanitize::sanitize_for_terminal;
use crate::repl::config::ComposeContext;
use crate::repl::error::ReplError;

/// Guard message printed when `:depends` is invoked outside Compose mode.
const GUARD_MSG: &str = "\
:depends is only available in compose mode
  Usage: demu --compose -f compose.yaml --service <name>
";

/// Execute the `:depends` command.
///
/// When `ctx` is `None` the guard message is printed.
/// When `ctx` is `Some`, a dependency tree is rendered starting from
/// `ctx.selected_service`.
pub fn execute(ctx: Option<&ComposeContext>, writer: &mut impl Write) -> Result<(), ReplError> {
    let io_err = |e: std::io::Error| ReplError::InvalidArguments {
        command: ":depends".to_string(),
        message: e.to_string(),
    };

    let ctx = match ctx {
        Some(c) => c,
        None => {
            write!(writer, "{GUARD_MSG}").map_err(io_err)?;
            return Ok(());
        }
    };

    let root = &ctx.selected_service;
    let services = &ctx.compose_file.services;

    // #63: Validate that the selected service exists in the map. If not,
    // output a clear warning rather than silently showing empty tree output
    // that looks identical to a valid leaf service.
    let safe_root = sanitize_for_terminal(root);
    if !services.contains_key(root.as_str()) {
        writeln!(
            writer,
            "warning: service '{safe_root}' not found in compose file"
        )
        .map_err(io_err)?;
        return Ok(());
    }

    // Print root service.
    writeln!(writer, "{safe_root}").map_err(io_err)?;

    // #64: Two-set DFS:
    // - `on_stack`: nodes on the current path from root → cycle detection.
    // - `fully_visited`: nodes whose subtrees have been fully expanded in any
    //   prior branch → print `[shared, see above]` instead of re-expanding.
    let mut on_stack: HashSet<String> = HashSet::new();
    let mut fully_visited: HashSet<String> = HashSet::new();
    on_stack.insert(root.clone());

    // Collect immediate dependencies of root, in original order.
    if let Some(svc) = services.get(root.as_str()) {
        for dep in &svc.depends_on {
            print_dep_tree(
                dep,
                services,
                &mut on_stack,
                &mut fully_visited,
                1,
                writer,
                &io_err,
            )?;
        }
    }

    Ok(())
}

/// Recursively print one level of the dependency tree.
///
/// `depth` controls the indentation level (1 = first level of children).
/// `on_stack` contains nodes on the current DFS path (cycle detection).
/// `fully_visited` contains nodes whose subtrees were already fully printed
/// in a prior branch; these are shown as `[shared, see above]` instead of
/// being re-expanded.
fn print_dep_tree(
    name: &str,
    services: &std::collections::BTreeMap<String, crate::model::compose::Service>,
    on_stack: &mut HashSet<String>,
    fully_visited: &mut HashSet<String>,
    depth: usize,
    writer: &mut impl Write,
    io_err: &impl Fn(std::io::Error) -> ReplError,
) -> Result<(), ReplError> {
    let indent = "  ".repeat(depth);
    let safe_name = sanitize_for_terminal(name);

    if on_stack.contains(name) {
        // Cycle detected — this node is already on the current path.
        writeln!(writer, "{indent}└─ {safe_name}  [cycle detected]").map_err(io_err)?;
        return Ok(());
    }

    if fully_visited.contains(name) {
        // Already fully expanded in a prior branch — avoid duplicate subtrees.
        writeln!(writer, "{indent}└─ {safe_name}  [shared, see above]").map_err(io_err)?;
        return Ok(());
    }

    writeln!(writer, "{indent}└─ {safe_name}").map_err(io_err)?;

    // Mark on the current path and recurse into dependencies.
    on_stack.insert(name.to_string());

    if let Some(svc) = services.get(name) {
        for dep in &svc.depends_on {
            print_dep_tree(
                dep,
                services,
                on_stack,
                fully_visited,
                depth + 1,
                writer,
                io_err,
            )?;
        }
    }

    // Remove from current path on backtrack, then mark as fully visited so
    // future branches that reach this node show `[shared, see above]`.
    on_stack.remove(name);
    fully_visited.insert(name.to_string());

    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::model::compose::{ComposeFile, Service, VolumeDefinition};
    use crate::repl::config::ComposeContext;
    use std::collections::BTreeMap;

    fn run(ctx: Option<&ComposeContext>) -> String {
        let mut buf = Vec::new();
        execute(ctx, &mut buf).expect("depends should not fail");
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
            out.contains(":depends is only available in compose mode"),
            "guard message missing; got: {out}"
        );
        assert!(
            out.contains("--compose"),
            "usage hint must mention --compose; got: {out}"
        );
    }

    // --- leaf service (no dependencies) ---

    #[test]
    fn leaf_service_shows_root_only() {
        let db = bare_service("db");
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![db]),
            selected_service: "db".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(
            out.trim() == "db",
            "leaf service should only print its name; got: {out}"
        );
    }

    // --- service with one dependency ---

    #[test]
    fn single_dependency_shows_tree_entry() {
        let db = bare_service("db");
        let api = Service {
            depends_on: vec!["db".to_string()],
            image: Some("myapp".to_string()),
            ..bare_service("api")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![api, db]),
            selected_service: "api".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(
            out.starts_with("api\n"),
            "root must be first line; got: {out}"
        );
        assert!(
            out.contains("└─ db"),
            "dependency must appear as tree entry; got: {out}"
        );
    }

    // --- service with two dependencies ---

    #[test]
    fn two_dependencies_both_appear() {
        let db = bare_service("db");
        let redis = bare_service("redis");
        let api = Service {
            depends_on: vec!["db".to_string(), "redis".to_string()],
            image: Some("myapp".to_string()),
            ..bare_service("api")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![api, db, redis]),
            selected_service: "api".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(out.contains("└─ db"), "db must appear; got: {out}");
        assert!(out.contains("└─ redis"), "redis must appear; got: {out}");
    }

    // --- multi-level tree ---

    #[test]
    fn multi_level_tree_indented_correctly() {
        // api → db → cache (3 levels)
        let cache = bare_service("cache");
        let db = Service {
            depends_on: vec!["cache".to_string()],
            image: Some("postgres".to_string()),
            ..bare_service("db")
        };
        let api = Service {
            depends_on: vec!["db".to_string()],
            image: Some("myapp".to_string()),
            ..bare_service("api")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![api, cache, db]),
            selected_service: "api".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(out.starts_with("api"), "root first; got: {out}");
        assert!(out.contains("└─ db"), "first level; got: {out}");
        // cache must appear indented at a deeper level than db
        let db_pos = out.find("└─ db").expect("db must be present");
        let cache_pos = out.find("└─ cache").expect("cache must be present");
        assert!(
            cache_pos > db_pos,
            "cache must appear after db; got:\n{out}"
        );
    }

    // --- cycle detection ---

    #[test]
    fn cycle_is_detected_and_marked() {
        // a → b → a (cycle)
        let a = Service {
            depends_on: vec!["b".to_string()],
            image: Some("img".to_string()),
            ..bare_service("a")
        };
        let b = Service {
            depends_on: vec!["a".to_string()],
            image: Some("img".to_string()),
            ..bare_service("b")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![a, b]),
            selected_service: "a".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(
            out.contains("[cycle detected]"),
            "cycle must be marked; got: {out}"
        );
    }

    // --- same service appears in multiple branches (diamond pattern) ---

    #[test]
    fn diamond_dependency_shows_shared_see_above() {
        // api → db AND api → cache; both → shared (diamond, not a cycle).
        // With the two-set DFS: shared appears fully under db (whichever comes
        // first alphabetically), then as `[shared, see above]` under cache.
        let shared = bare_service("shared");
        let db = Service {
            depends_on: vec!["shared".to_string()],
            image: Some("postgres".to_string()),
            ..bare_service("db")
        };
        let cache = Service {
            depends_on: vec!["shared".to_string()],
            image: Some("redis".to_string()),
            ..bare_service("cache")
        };
        let api = Service {
            depends_on: vec!["db".to_string(), "cache".to_string()],
            image: Some("myapp".to_string()),
            ..bare_service("api")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![api, cache, db, shared]),
            selected_service: "api".to_string(),
        };
        let out = run(Some(&ctx));
        // No cycle must be reported.
        assert!(
            !out.contains("[cycle detected]"),
            "diamond pattern must not trigger cycle detection; got:\n{out}"
        );
        // shared must appear exactly once as a normal entry and once as [shared, see above].
        let full_count = out.matches("└─ shared").count();
        let shared_above = out.contains("[shared, see above]");
        assert_eq!(
            full_count, 2,
            "shared must appear twice total (once full, once marker); got:\n{out}"
        );
        assert!(
            shared_above,
            "second occurrence of shared must show '[shared, see above]'; got:\n{out}"
        );
    }

    // --- #63: selected_service not in map ---

    #[test]
    fn selected_service_not_in_map_shows_warning() {
        let db = bare_service("db");
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![db]),
            selected_service: "typo_service".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(
            out.contains("warning:"),
            "must show a warning when selected service is not found; got: {out}"
        );
        assert!(
            out.contains("typo_service"),
            "warning must mention the missing service name; got: {out}"
        );
        // Must NOT show a bare root line that looks like a valid leaf.
        assert!(
            !out.trim().eq("typo_service"),
            "must not output bare name as if it were a valid leaf; got: {out}"
        );
    }

    // --- ANSI sanitization ---

    #[test]
    fn ansi_escape_in_service_name_is_stripped() {
        let evil_dep = Service {
            image: Some("img".to_string()),
            ..bare_service("dep\x1b[2J")
        };
        let api = Service {
            depends_on: vec!["dep\x1b[2J".to_string()],
            image: Some("myapp".to_string()),
            ..bare_service("api")
        };
        let ctx = ComposeContext {
            compose_file: compose_with_services(vec![api, evil_dep]),
            selected_service: "api".to_string(),
        };
        let out = run(Some(&ctx));
        assert!(
            !out.contains('\x1b'),
            "ANSI escapes must be stripped; got: {out:?}"
        );
    }
}
