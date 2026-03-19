// Session-level configuration for the REPL.
//
// `ReplConfig` holds the fixed session metadata that the REPL needs to support
// `:reload`. These values are immutable after construction and are separate from
// `PreviewState`, which holds simulation output.

use std::path::PathBuf;

/// Session-level configuration for the REPL.
///
/// Holds the fixed session metadata (Dockerfile path, build context directory)
/// that the REPL needs to support `:reload`. These values are immutable after
/// construction and are separate from `PreviewState`, which holds simulation output.
pub struct ReplConfig {
    /// Absolute path to the Dockerfile being previewed.
    pub dockerfile_path: PathBuf,
    /// Absolute path to the build context directory (parent of the Dockerfile).
    ///
    /// Derived automatically from `dockerfile_path.parent()` by `ReplConfig::new`.
    /// If a different context directory is required, use `ReplConfig::with_context`.
    pub context_dir: PathBuf,
}

impl ReplConfig {
    /// Construct a `ReplConfig` from the Dockerfile path alone.
    ///
    /// The build context directory is derived as `dockerfile_path.parent()`, which
    /// is the standard Docker build context for a project Dockerfile. If the path
    /// has no parent component (an edge case for bare filenames), `/` is used.
    ///
    /// `dockerfile_path` should be an absolute, canonicalized path.
    pub fn new(dockerfile_path: PathBuf) -> Self {
        let context_dir = dockerfile_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self {
            dockerfile_path,
            context_dir,
        }
    }

    /// Construct a `ReplConfig` with an explicit build context directory.
    ///
    /// Use this when the context directory differs from the Dockerfile's parent —
    /// for example, when a `--context` flag or Compose configuration specifies
    /// a different root.
    pub fn with_context(dockerfile_path: PathBuf, context_dir: PathBuf) -> Self {
        Self {
            dockerfile_path,
            context_dir,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- ReplConfig::new derives context_dir from parent ---

    #[test]
    fn repl_config_new_derives_context_dir_from_parent() {
        let df_path = PathBuf::from("/project/Dockerfile");
        let config = ReplConfig::new(df_path.clone());
        assert_eq!(config.dockerfile_path, df_path);
        assert_eq!(config.context_dir, PathBuf::from("/project"));
    }

    #[test]
    fn repl_config_new_nested_path_derives_correct_parent() {
        let df_path = PathBuf::from("/home/user/app/Dockerfile");
        let config = ReplConfig::new(df_path);
        assert_eq!(config.context_dir, PathBuf::from("/home/user/app"));
    }

    // --- ReplConfig::with_context accepts explicit context directory ---

    #[test]
    fn repl_config_with_context_stores_custom_context_dir() {
        let df_path = PathBuf::from("/project/docker/Dockerfile");
        let ctx_dir = PathBuf::from("/project");
        let config = ReplConfig::with_context(df_path.clone(), ctx_dir.clone());
        assert_eq!(config.dockerfile_path, df_path);
        assert_eq!(config.context_dir, ctx_dir);
    }

    // --- Public fields are directly accessible ---

    #[test]
    fn repl_config_fields_are_accessible() {
        let config = ReplConfig::new(PathBuf::from("/app/Dockerfile"));
        let _ = &config.dockerfile_path;
        let _ = &config.context_dir;
        assert_eq!(config.dockerfile_path, PathBuf::from("/app/Dockerfile"));
        assert_eq!(config.context_dir, PathBuf::from("/app"));
    }
}
