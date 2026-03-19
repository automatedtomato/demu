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

/// A registry of all completed build stages, indexed by name and by number.
///
/// Each stage is stored under two keys: its alias (e.g. `"builder"`) and its
/// zero-based numeric index as a string (e.g. `"0"`, `"1"`). Stages without
/// an alias are stored only by numeric index.
///
/// Uses `BTreeMap` for deterministic key ordering in tests and output.
#[derive(Debug, Clone, Default)]
pub struct StageRegistry {
    /// Map from stage key (numeric index string or alias) to `PreviewState`.
    ///
    /// An aliased stage is stored under both its numeric index and its alias.
    /// Numeric-only stages are stored only under their index string.
    stages: BTreeMap<String, PreviewState>,
}

impl StageRegistry {
    /// Insert a completed stage.
    ///
    /// Always stores the stage by `index` (as a string). Also stores it by
    /// `alias` when `alias` is `Some`. If the alias key already exists, it
    /// is overwritten (last-write wins, matching Dockerfile semantics where
    /// two stages with the same alias is an error — we just handle it gracefully).
    pub fn insert(&mut self, index: usize, alias: Option<&str>, state: PreviewState) {
        // Store by numeric index first. The clone is needed when we also store by alias.
        let index_key = index.to_string();
        if let Some(alias_str) = alias {
            // Insert under the alias key, cloning state so the index key can own it too.
            self.stages.insert(alias_str.to_string(), state.clone());
            self.stages.insert(index_key, state);
        } else {
            self.stages.insert(index_key, state);
        }
    }

    /// Look up a stage by name or numeric index string.
    ///
    /// Returns `None` when no stage matches `key`.
    pub fn get(&self, key: &str) -> Option<&PreviewState> {
        self.stages.get(key)
    }

    /// Return a sorted list of all stage keys (aliases and numeric indices).
    ///
    /// The list is sorted lexicographically because the underlying store is a `BTreeMap`.
    pub fn keys(&self) -> Vec<String> {
        self.stages.keys().cloned().collect()
    }

    /// Return the total number of unique stages stored (by numeric index).
    ///
    /// Stages stored under both alias and index count as one stage.
    /// This counts only the numeric-index keys (pure digits), which represent
    /// unique stages regardless of how many aliases they have.
    pub fn len(&self) -> usize {
        self.stages
            .keys()
            .filter(|k| k.parse::<usize>().is_ok())
            .count()
    }

    /// Return `true` when no stages have been recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

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

    /// Return the simulated binary prefix for the first manager that has `cmd`
    /// installed, using the canonical priority order: apt → apk → pip → npm → go.
    ///
    /// - `apt` / `apk` → `"/usr/bin"` (system path)
    /// - `pip` / `npm` / `go` → `"/usr/local/bin"` (user-local path)
    /// - Not found in any manager → `None`
    ///
    /// This centralises the priority logic so that `which` and any future
    /// consumers (e.g. issue #23) share a single, consistent implementation.
    pub fn which_prefix(&self, cmd: &str) -> Option<&'static str> {
        if self.apt.contains(cmd) || self.apk.contains(cmd) {
            Some("/usr/bin")
        } else if self.pip.contains(cmd) || self.npm.contains(cmd) || self.go_pkgs.contains(cmd) {
            Some("/usr/local/bin")
        } else {
            None
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

    // --- InstalledRegistry::which_prefix ---

    #[test]
    fn which_prefix_apt_returns_usr_bin() {
        let mut reg = InstalledRegistry::default();
        reg.record("apt", "curl".to_string());
        assert_eq!(reg.which_prefix("curl"), Some("/usr/bin"));
    }

    #[test]
    fn which_prefix_apk_returns_usr_bin() {
        let mut reg = InstalledRegistry::default();
        reg.record("apk", "bash".to_string());
        assert_eq!(reg.which_prefix("bash"), Some("/usr/bin"));
    }

    #[test]
    fn which_prefix_pip_returns_usr_local_bin() {
        let mut reg = InstalledRegistry::default();
        reg.record("pip", "flask".to_string());
        assert_eq!(reg.which_prefix("flask"), Some("/usr/local/bin"));
    }

    #[test]
    fn which_prefix_npm_returns_usr_local_bin() {
        let mut reg = InstalledRegistry::default();
        reg.record("npm", "eslint".to_string());
        assert_eq!(reg.which_prefix("eslint"), Some("/usr/local/bin"));
    }

    #[test]
    fn which_prefix_go_returns_usr_local_bin() {
        let mut reg = InstalledRegistry::default();
        reg.record("go", "gopls".to_string());
        assert_eq!(reg.which_prefix("gopls"), Some("/usr/local/bin"));
    }

    #[test]
    fn which_prefix_not_found_returns_none() {
        let reg = InstalledRegistry::default();
        assert_eq!(reg.which_prefix("nonexistent"), None);
    }

    #[test]
    fn which_prefix_apt_beats_pip_for_same_name() {
        // apt is searched before pip — apt wins regardless of pip also having the name.
        let mut reg = InstalledRegistry::default();
        reg.record("pip", "git".to_string());
        reg.record("apt", "git".to_string());
        assert_eq!(reg.which_prefix("git"), Some("/usr/bin"));
    }

    #[test]
    fn which_prefix_apk_beats_pip_for_same_name() {
        // apk is searched in the same group as apt, before pip.
        let mut reg = InstalledRegistry::default();
        reg.record("pip", "bash".to_string());
        reg.record("apk", "bash".to_string());
        assert_eq!(reg.which_prefix("bash"), Some("/usr/bin"));
    }

    #[test]
    fn which_prefix_pip_beats_npm_for_same_name() {
        // pip is listed first in the local-bin group — pip wins over npm.
        let mut reg = InstalledRegistry::default();
        reg.record("npm", "requests".to_string());
        reg.record("pip", "requests".to_string());
        assert_eq!(reg.which_prefix("requests"), Some("/usr/local/bin"));
    }

    // --- StageRegistry ---

    #[test]
    fn stage_registry_is_empty_on_default() {
        let reg = StageRegistry::default();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn stage_registry_is_not_empty_after_insert() {
        let mut reg = StageRegistry::default();
        reg.insert(0, None, PreviewState::default());
        assert!(!reg.is_empty());
    }

    #[test]
    fn stage_registry_insert_by_index_only_when_no_alias() {
        let mut reg = StageRegistry::default();
        reg.insert(0, None, PreviewState::default());
        // Should be retrievable by index string "0".
        assert!(reg.get("0").is_some());
        // Should NOT be stored under any alias key (only numeric key exists).
        assert_eq!(reg.keys(), vec!["0"]);
    }

    #[test]
    fn stage_registry_insert_by_index_and_alias() {
        let mut reg = StageRegistry::default();
        let mut s = PreviewState::default();
        s.env.insert("KEY".to_string(), "val".to_string());
        reg.insert(0, Some("builder"), s);
        // Should be stored under both "0" and "builder".
        assert!(reg.get("0").is_some());
        assert!(reg.get("builder").is_some());
    }

    #[test]
    fn stage_registry_get_by_index_string() {
        let mut reg = StageRegistry::default();
        let mut s = PreviewState::default();
        s.env.insert("STAGE".to_string(), "zero".to_string());
        reg.insert(0, None, s);
        let retrieved = reg.get("0").expect("stage 0 must exist");
        assert_eq!(retrieved.env.get("STAGE").map(String::as_str), Some("zero"));
    }

    #[test]
    fn stage_registry_get_by_alias() {
        let mut reg = StageRegistry::default();
        let mut s = PreviewState::default();
        s.env.insert("WHO".to_string(), "builder".to_string());
        reg.insert(1, Some("builder"), s);
        let retrieved = reg.get("builder").expect("stage 'builder' must exist");
        assert_eq!(
            retrieved.env.get("WHO").map(String::as_str),
            Some("builder")
        );
    }

    #[test]
    fn stage_registry_get_returns_none_for_unknown_key() {
        let reg = StageRegistry::default();
        assert!(reg.get("nonexistent").is_none());
        assert!(reg.get("99").is_none());
    }

    #[test]
    fn stage_registry_keys_returns_sorted_keys() {
        let mut reg = StageRegistry::default();
        reg.insert(0, Some("builder"), PreviewState::default());
        reg.insert(1, Some("runner"), PreviewState::default());
        let keys = reg.keys();
        // BTreeMap gives lexicographic order: "0", "1", "builder", "runner"
        assert_eq!(keys, vec!["0", "1", "builder", "runner"]);
    }

    #[test]
    fn stage_registry_len_counts_unique_stages_by_index() {
        let mut reg = StageRegistry::default();
        // Insert two stages: one with alias, one without.
        reg.insert(0, Some("builder"), PreviewState::default());
        reg.insert(1, None, PreviewState::default());
        // len() counts by numeric index keys only → 2 unique stages.
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn stage_registry_alias_and_index_share_same_state() {
        let mut reg = StageRegistry::default();
        let mut s = PreviewState::default();
        s.cwd = PathBuf::from("/build");
        reg.insert(0, Some("builder"), s);
        // Both keys should reflect the same cwd.
        assert_eq!(reg.get("0").expect("by index").cwd, PathBuf::from("/build"));
        assert_eq!(
            reg.get("builder").expect("by alias").cwd,
            PathBuf::from("/build")
        );
    }

    // --- Clone ---

    #[test]
    fn preview_state_clone_is_independent() {
        let mut state = PreviewState::default();
        let mut clone = state.clone();
        // Mutating the clone must not affect the original.
        clone.cwd = PathBuf::from("/app");
        assert_eq!(state.cwd, PathBuf::from("/"));

        state
            .warnings
            .push(crate::model::warning::Warning::EmptyBaseImage {
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
