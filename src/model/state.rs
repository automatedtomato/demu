// Top-level simulation state shared across all engine passes.
//
// `PreviewState` is the single mutable value that the engine updates as it
// processes each Dockerfile instruction. It captures the virtual filesystem,
// environment variables, installed packages, instruction history, layer
// summaries, accumulated warnings, and the current working directory.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::fs::VirtualFs;
use super::warning::Warning;

/// Registry of packages that the simulation has recorded as installed.
///
/// Uses `BTreeSet` for each manager so that `list()` always returns packages
/// in alphabetical order without additional sorting.
#[derive(Debug, Clone, Default)]
pub struct InstalledRegistry {
    /// Packages recorded by `apt` / `apt-get`.
    pub apt: BTreeSet<String>,
    /// Packages recorded by `pip` / `pip3`.
    pub pip: BTreeSet<String>,
    /// Packages recorded by `npm`.
    pub npm: BTreeSet<String>,
    /// Packages recorded by `go install`.
    pub go_pkgs: BTreeSet<String>,
    /// Packages recorded by `apk add`.
    pub apk: BTreeSet<String>,
}

impl InstalledRegistry {
    /// Record a package as installed under the given manager name.
    ///
    /// Recognised manager names: `"apt"`, `"pip"`, `"npm"`, `"go"`, `"apk"`.
    ///
    /// Returns `true` if the manager was recognised and the package was recorded,
    /// `false` if the manager is unknown. Callers should emit a `Warning` when
    /// this returns `false` so the user sees that the install was not modeled.
    pub fn record(&mut self, manager: &str, package: String) -> bool {
        match manager {
            "apt" => {
                self.apt.insert(package);
                true
            }
            "pip" => {
                self.pip.insert(package);
                true
            }
            "npm" => {
                self.npm.insert(package);
                true
            }
            "go" => {
                self.go_pkgs.insert(package);
                true
            }
            "apk" => {
                self.apk.insert(package);
                true
            }
            // Unknown manager — caller is responsible for emitting a Warning.
            _ => false,
        }
    }

    /// Return a sorted list of packages recorded for the given manager.
    ///
    /// Returns an empty vec for unrecognised manager names.
    pub fn list(&self, manager: &str) -> Vec<String> {
        match manager {
            "apt" => self.apt.iter().cloned().collect(),
            "pip" => self.pip.iter().cloned().collect(),
            "npm" => self.npm.iter().cloned().collect(),
            "go" => self.go_pkgs.iter().cloned().collect(),
            "apk" => self.apk.iter().cloned().collect(),
            _ => vec![],
        }
    }
}

/// A single entry in the instruction history timeline.
///
/// The REPL `:history` command displays these entries to show the user what
/// the engine processed and what observable effect it had.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    /// Source line number (1-based) from the Dockerfile.
    pub line: usize,
    /// The raw instruction text (e.g. "COPY . /app").
    pub instruction: String,
    /// A human-readable summary of what the engine did (e.g. "set cwd to /app").
    pub effect: String,
}

/// A summary of what changed during one Dockerfile instruction ("layer").
///
/// The REPL `:layers` command displays these to give a Docker-like layer view.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerSummary {
    /// The instruction type keyword (e.g. "COPY", "RUN", "ENV").
    pub instruction_type: String,
    /// Paths that were created, modified, or removed in the virtual filesystem.
    pub files_changed: Vec<PathBuf>,
    /// Environment variable assignments applied (key, value pairs).
    pub env_changed: Vec<(String, String)>,
}

/// The complete mutable state of the simulation at any point in time.
///
/// The engine reads this state before each instruction and produces an updated
/// value after each instruction. The REPL holds the current `PreviewState` and
/// passes it to engine functions to compute previews.
#[derive(Debug, Clone)]
pub struct PreviewState {
    /// The current working directory inside the simulated container.
    pub cwd: PathBuf,

    /// Environment variable map, ordered lexicographically by key.
    pub env: BTreeMap<String, String>,

    /// The virtual filesystem.
    pub fs: VirtualFs,

    /// Registry of packages the simulation has recorded as installed.
    pub installed: InstalledRegistry,

    /// Ordered list of instruction history entries.
    pub history: Vec<HistoryEntry>,

    /// Ordered list of layer summaries, one per instruction processed.
    pub layers: Vec<LayerSummary>,

    /// Warnings accumulated during processing (non-fatal).
    pub warnings: Vec<Warning>,

    /// The currently active build stage alias, if any (used for multi-stage builds).
    pub active_stage: Option<String>,
}

impl Default for PreviewState {
    /// Construct a `PreviewState` that represents an empty container.
    ///
    /// The default working directory is `/` (the container root), matching
    /// Docker's own starting state before any WORKDIR instruction.
    fn default() -> Self {
        Self {
            cwd: PathBuf::from("/"),
            env: BTreeMap::new(),
            fs: VirtualFs::new(),
            installed: InstalledRegistry::default(),
            history: Vec::new(),
            layers: Vec::new(),
            warnings: Vec::new(),
            active_stage: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- PreviewState::default ---

    #[test]
    fn default_cwd_is_root() {
        let state = PreviewState::default();
        assert_eq!(state.cwd, PathBuf::from("/"));
    }

    #[test]
    fn default_env_is_empty() {
        let state = PreviewState::default();
        assert!(state.env.is_empty());
    }

    #[test]
    fn default_fs_is_empty() {
        let state = PreviewState::default();
        // VirtualFs::iter gives all nodes; none should exist.
        assert_eq!(state.fs.iter().count(), 0);
    }

    #[test]
    fn default_history_is_empty() {
        let state = PreviewState::default();
        assert!(state.history.is_empty());
    }

    #[test]
    fn default_layers_is_empty() {
        let state = PreviewState::default();
        assert!(state.layers.is_empty());
    }

    #[test]
    fn default_warnings_is_empty() {
        let state = PreviewState::default();
        assert!(state.warnings.is_empty());
    }

    #[test]
    fn default_active_stage_is_none() {
        let state = PreviewState::default();
        assert!(state.active_stage.is_none());
    }

    // --- InstalledRegistry::default ---

    #[test]
    fn installed_registry_default_all_sets_empty() {
        let reg = InstalledRegistry::default();
        assert!(reg.apt.is_empty());
        assert!(reg.pip.is_empty());
        assert!(reg.npm.is_empty());
        assert!(reg.go_pkgs.is_empty());
        assert!(reg.apk.is_empty());
    }

    // --- InstalledRegistry::record and list ---

    #[test]
    fn record_apt_package_then_list_returns_it() {
        let mut reg = InstalledRegistry::default();
        reg.record("apt", "curl".to_string());
        assert_eq!(reg.list("apt"), vec!["curl"]);
    }

    #[test]
    fn record_pip_package_then_list_returns_it() {
        let mut reg = InstalledRegistry::default();
        reg.record("pip", "requests".to_string());
        assert_eq!(reg.list("pip"), vec!["requests"]);
    }

    #[test]
    fn record_npm_package_then_list_returns_it() {
        let mut reg = InstalledRegistry::default();
        reg.record("npm", "typescript".to_string());
        assert_eq!(reg.list("npm"), vec!["typescript"]);
    }

    #[test]
    fn record_go_package_then_list_returns_it() {
        let mut reg = InstalledRegistry::default();
        reg.record("go", "golang.org/x/tools".to_string());
        assert_eq!(reg.list("go"), vec!["golang.org/x/tools"]);
    }

    #[test]
    fn record_apk_package_then_list_returns_it() {
        let mut reg = InstalledRegistry::default();
        reg.record("apk", "busybox".to_string());
        assert_eq!(reg.list("apk"), vec!["busybox"]);
    }

    #[test]
    fn list_returns_sorted_results() {
        let mut reg = InstalledRegistry::default();
        // Insert in reverse alphabetical order.
        reg.record("apt", "wget".to_string());
        reg.record("apt", "curl".to_string());
        reg.record("apt", "bash".to_string());
        // BTreeSet guarantees alphabetical order.
        assert_eq!(reg.list("apt"), vec!["bash", "curl", "wget"]);
    }

    #[test]
    fn record_multiple_managers_do_not_cross_contaminate() {
        let mut reg = InstalledRegistry::default();
        reg.record("apt", "curl".to_string());
        reg.record("pip", "flask".to_string());
        // apt list must not contain pip packages and vice versa.
        assert_eq!(reg.list("apt"), vec!["curl"]);
        assert_eq!(reg.list("pip"), vec!["flask"]);
        assert!(reg.list("npm").is_empty());
    }

    #[test]
    fn record_duplicate_package_is_idempotent() {
        let mut reg = InstalledRegistry::default();
        reg.record("apt", "git".to_string());
        reg.record("apt", "git".to_string());
        // BTreeSet deduplicates — only one entry.
        assert_eq!(reg.list("apt"), vec!["git"]);
    }

    #[test]
    fn record_unknown_manager_returns_false() {
        let mut reg = InstalledRegistry::default();
        // Returns false so the caller can emit a Warning.
        let accepted = reg.record("brew", "htop".to_string());
        assert!(!accepted);
        // Package was not recorded anywhere.
        assert!(reg.list("brew").is_empty());
    }

    #[test]
    fn record_known_manager_returns_true() {
        let mut reg = InstalledRegistry::default();
        let accepted = reg.record("apt", "curl".to_string());
        assert!(accepted);
    }

    #[test]
    fn list_unknown_manager_returns_empty_vec() {
        let reg = InstalledRegistry::default();
        assert!(reg.list("brew").is_empty());
    }

    // --- Clone ---

    #[test]
    fn preview_state_clone_is_independent() {
        let mut state = PreviewState::default();
        let mut clone = state.clone();
        // Mutating the clone must not affect the original.
        clone.cwd = PathBuf::from("/app");
        assert_eq!(state.cwd, PathBuf::from("/"));

        state.warnings.push(crate::model::warning::Warning::EmptyBaseImage {
            image: "scratch".to_string(),
        });
        assert!(clone.warnings.is_empty());
    }

    // --- HistoryEntry construction ---

    #[test]
    fn history_entry_stores_all_fields() {
        let entry = HistoryEntry {
            line: 3,
            instruction: "WORKDIR /app".to_string(),
            effect: "set cwd to /app".to_string(),
        };
        assert_eq!(entry.line, 3);
        assert_eq!(entry.instruction, "WORKDIR /app");
        assert_eq!(entry.effect, "set cwd to /app");
    }

    // --- LayerSummary construction ---

    #[test]
    fn layer_summary_stores_all_fields() {
        let layer = LayerSummary {
            instruction_type: "COPY".to_string(),
            files_changed: vec![PathBuf::from("/app/main.rs")],
            env_changed: vec![],
        };
        assert_eq!(layer.instruction_type, "COPY");
        assert_eq!(layer.files_changed.len(), 1);
        assert!(layer.env_changed.is_empty());
    }
}
