// Session-level configuration for the REPL.
//
// `ReplConfig` holds the fixed session metadata that the REPL needs to support
// `:reload`. These values are immutable after construction and are separate from
// `PreviewState`, which holds simulation output.

use std::path::PathBuf;

use crate::model::compose::ComposeFile;

/// Holds the Compose file and the currently selected service name for sessions
/// that were started with `--compose`.
///
/// Stored in `ReplConfig` so that `:reload` can re-parse the Compose file and
/// future commands such as `:services`, `:mounts`, and `:depends` can access
/// the full service graph.
pub struct ComposeContext {
    /// The fully parsed Compose file.
    pub compose_file: ComposeFile,
    /// The name of the service selected via `--service`.
    pub selected_service: String,
}

/// Session-level configuration for the REPL.
///
/// Holds the fixed session metadata (Dockerfile path, build context directory,
/// optional stage selection, and optional Compose context) that the REPL needs
/// to support `:reload`. These values are immutable after construction and are
/// separate from `PreviewState`, which holds simulation output.
pub struct ReplConfig {
    /// Absolute path to the Dockerfile or Compose file being previewed.
    pub dockerfile_path: PathBuf,
    /// Absolute path to the build context directory (parent of the file).
    ///
    /// Derived automatically from `dockerfile_path.parent()` by `ReplConfig::new`.
    /// If a different context directory is required, use `ReplConfig::with_context`.
    pub context_dir: PathBuf,
    /// The stage name or index selected via `--stage` at startup.
    ///
    /// `None` means "use the final stage" (default behavior).
    /// `:reload` uses this to re-apply the same selection after re-running the engine.
    pub selected_stage: Option<String>,
    /// Present when the session was started in Compose mode (`--compose`).
    ///
    /// `None` in single-Dockerfile mode. When `Some`, REPL commands such as
    /// `:services`, `:mounts`, and `:depends` are available.
    pub compose_context: Option<ComposeContext>,
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
            selected_stage: None,
            compose_context: None,
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
            selected_stage: None,
            compose_context: None,
        }
    }

    /// Set the stage selection for this config.
    ///
    /// Called from `main` after `--stage` is parsed so that `:reload` can
    /// re-apply the same stage selection each time the Dockerfile is reloaded.
    pub fn with_selected_stage(mut self, stage: Option<String>) -> Self {
        self.selected_stage = stage;
        self
    }

    /// Attach a `ComposeContext` to this config.
    ///
    /// Called from `main` when `--compose` is set. Makes the parsed Compose file
    /// and selected service name available to the REPL for Compose-aware commands.
    pub fn with_compose_context(mut self, ctx: Option<ComposeContext>) -> Self {
        self.compose_context = ctx;
        self
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::compose::ComposeFile;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn empty_compose_file() -> ComposeFile {
        ComposeFile {
            services: BTreeMap::new(),
            volumes: BTreeMap::new(),
        }
    }

    // --- ComposeContext is stored and retrieved via builder ---

    #[test]
    fn repl_config_compose_context_defaults_to_none() {
        let config = ReplConfig::new(PathBuf::from("/project/Dockerfile"));
        assert!(config.compose_context.is_none());
    }

    #[test]
    fn repl_config_with_context_compose_context_defaults_to_none() {
        let config = ReplConfig::with_context(
            PathBuf::from("/project/docker/Dockerfile"),
            PathBuf::from("/project"),
        );
        assert!(config.compose_context.is_none());
    }

    #[test]
    fn repl_config_with_compose_context_stores_value() {
        let ctx = ComposeContext {
            compose_file: empty_compose_file(),
            selected_service: "api".to_string(),
        };
        let config =
            ReplConfig::new(PathBuf::from("/project/compose.yaml")).with_compose_context(Some(ctx));
        let stored = config.compose_context.expect("should be Some");
        assert_eq!(stored.selected_service, "api");
    }

    #[test]
    fn repl_config_with_compose_context_none_clears() {
        let ctx = ComposeContext {
            compose_file: empty_compose_file(),
            selected_service: "api".to_string(),
        };
        let config = ReplConfig::new(PathBuf::from("/project/compose.yaml"))
            .with_compose_context(Some(ctx))
            .with_compose_context(None);
        assert!(config.compose_context.is_none());
    }

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
