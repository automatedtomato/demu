// Path resolution utilities for REPL command handlers.
//
// All path math in the REPL is pure (no filesystem I/O). We compute the
// canonical absolute path by normalising `.` and `..` components against
// the current working directory without touching the host filesystem.

use std::path::{Component, Path, PathBuf};

/// Resolve `input` relative to `cwd`, returning a normalised absolute path.
///
/// Resolution rules:
/// - If `input` starts with `/`, treat it as an absolute path (ignore `cwd`).
/// - Otherwise, join `cwd` with `input` to form the candidate path.
/// - Normalise the resulting path by processing each component in order:
///   - `.` components are discarded.
///   - `..` components pop the last accumulated component, but never go above `/`.
///   - Normal components are appended.
///
/// The result is always absolute (starts with `/`). This is pure path
/// arithmetic — it does not check whether the path exists in any filesystem.
pub fn resolve_path(cwd: &Path, input: &str) -> PathBuf {
    let base = if input.starts_with('/') {
        PathBuf::from(input)
    } else {
        cwd.join(input)
    };

    normalize_path(&base)
}

/// Normalise an absolute path by resolving `.` and `..` components.
///
/// This operates purely on path components without accessing the host
/// filesystem. `..` above the root stays at `/`.
fn normalize_path(path: &Path) -> PathBuf {
    let mut components: Vec<&str> = Vec::new();

    for component in path.components() {
        match component {
            // Root slash — initialise the stack (will be prepended at the end).
            Component::RootDir => {}
            // Current directory — no-op.
            Component::CurDir => {}
            // Parent directory — pop one level, but never go above root.
            Component::ParentDir => {
                components.pop();
            }
            // Normal path segment — push onto the stack.
            Component::Normal(seg) => {
                if let Some(s) = seg.to_str() {
                    components.push(s);
                }
            }
            // Prefix (Windows drive letters etc.) — not applicable on Linux.
            Component::Prefix(_) => {}
        }
    }

    // Rebuild as an absolute path.
    let mut result = PathBuf::from("/");
    for seg in components {
        result.push(seg);
    }
    result
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    // --- Absolute passthrough ---

    #[test]
    fn absolute_input_ignores_cwd() {
        let cwd = Path::new("/home/user");
        assert_eq!(resolve_path(cwd, "/etc/hosts"), PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn absolute_root_resolves_to_root() {
        let cwd = Path::new("/app");
        assert_eq!(resolve_path(cwd, "/"), PathBuf::from("/"));
    }

    // --- Relative join ---

    #[test]
    fn relative_input_joins_with_cwd() {
        let cwd = Path::new("/app");
        assert_eq!(resolve_path(cwd, "src"), PathBuf::from("/app/src"));
    }

    #[test]
    fn relative_from_root_cwd() {
        let cwd = Path::new("/");
        assert_eq!(resolve_path(cwd, "etc"), PathBuf::from("/etc"));
    }

    #[test]
    fn relative_multi_segment() {
        let cwd = Path::new("/app");
        assert_eq!(
            resolve_path(cwd, "src/main.rs"),
            PathBuf::from("/app/src/main.rs")
        );
    }

    // --- Dot normalisation ---

    #[test]
    fn single_dot_resolves_to_cwd() {
        let cwd = Path::new("/app");
        assert_eq!(resolve_path(cwd, "."), PathBuf::from("/app"));
    }

    #[test]
    fn dot_slash_prefix_is_discarded() {
        let cwd = Path::new("/app");
        assert_eq!(resolve_path(cwd, "./src"), PathBuf::from("/app/src"));
    }

    // --- Double-dot normalisation ---

    #[test]
    fn dotdot_moves_up_one_level() {
        let cwd = Path::new("/app/src");
        assert_eq!(resolve_path(cwd, ".."), PathBuf::from("/app"));
    }

    #[test]
    fn dotdot_at_root_stays_at_root() {
        let cwd = Path::new("/");
        assert_eq!(resolve_path(cwd, ".."), PathBuf::from("/"));
    }

    #[test]
    fn dotdot_past_root_stays_at_root() {
        let cwd = Path::new("/");
        // Multiple .. from root must clamp at /.
        assert_eq!(resolve_path(cwd, "../../.."), PathBuf::from("/"));
    }

    #[test]
    fn dotdot_from_single_component_path_goes_to_root() {
        let cwd = Path::new("/app");
        assert_eq!(resolve_path(cwd, ".."), PathBuf::from("/"));
    }

    #[test]
    fn dotdot_then_forward() {
        let cwd = Path::new("/app/src");
        assert_eq!(resolve_path(cwd, "../lib"), PathBuf::from("/app/lib"));
    }

    // --- Double slashes and trailing slashes ---

    #[test]
    fn trailing_slash_resolves_correctly() {
        let cwd = Path::new("/app");
        // /etc/hosts/ should normalise to /etc/hosts
        let result = resolve_path(cwd, "/etc/hosts/");
        // Path normalisation removes trailing slash components.
        assert_eq!(result, PathBuf::from("/etc/hosts"));
    }

    // --- Absolute path with dots ---

    #[test]
    fn absolute_with_dot_normalised() {
        let cwd = Path::new("/anywhere");
        assert_eq!(resolve_path(cwd, "/app/./src"), PathBuf::from("/app/src"));
    }

    #[test]
    fn absolute_with_dotdot_normalised() {
        let cwd = Path::new("/anywhere");
        assert_eq!(
            resolve_path(cwd, "/app/src/../lib"),
            PathBuf::from("/app/lib")
        );
    }
}
