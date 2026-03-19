// RUN instruction handler for the simulation engine.
//
// Simulates a safe subset of shell commands (mkdir, touch, rm, mv, cp) by
// mutating the VirtualFs directly. Commands outside the modeled subset receive
// an UnmodeledRunCommand warning so the user can see what was skipped.
//
// `&&`-chain and `;`-separated commands are split into individual sub-commands,
// each dispatched separately. This gives the user a per-sub-command trace and
// allows mixed chains like `mkdir -p /app && touch /app/main.rs` to work.

use std::path::{Component, Path, PathBuf};

use super::copy::ensure_ancestors;
use crate::model::{
    fs::{DirNode, FileNode, FsNode},
    provenance::{Provenance, ProvenanceSource},
    state::{LayerSummary, PreviewState},
    warning::Warning,
};

// ─── Internal types ───────────────────────────────────────────────────────────

/// Accumulated changes produced by a single sub-command dispatch.
///
/// `files_changed` is populated by filesystem-mutating commands (mkdir, touch,
/// rm, mv, cp). `env_changed` is reserved for future env-mutating commands
/// such as `export` or inline variable assignments.
struct SubCommandResult {
    /// Filesystem paths created, modified, or removed by the sub-command.
    files_changed: Vec<PathBuf>,
    /// Environment variable assignments produced by the sub-command.
    env_changed: Vec<(String, String)>,
}

impl SubCommandResult {
    /// Construct an empty result (no filesystem or environment changes).
    fn empty() -> Self {
        Self {
            files_changed: vec![],
            env_changed: vec![],
        }
    }
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Split a sub-command string into argv-style tokens by whitespace.
///
/// Shell quoting is not modeled — this is consistent with the `split_commands`
/// limitation documented above.
fn parse_argv(sub_cmd: &str) -> Vec<&str> {
    sub_cmd.split_whitespace().collect()
}

/// Resolve a raw path string against the current working directory.
///
/// Absolute paths are treated as-is; relative paths are joined to `cwd`. The
/// result is normalized: `.` components are discarded, `..` components pop one
/// level (clamped at `/`). This matches `repl::path::resolve_path` semantics so
/// the engine and REPL produce consistent paths.
fn resolve_path(cwd: &Path, raw: &str) -> PathBuf {
    let base = if raw.starts_with('/') {
        PathBuf::from(raw)
    } else {
        cwd.join(raw)
    };
    normalize_path(&base)
}

/// Normalize an absolute path by resolving `.` and `..` components.
///
/// Pure path arithmetic — does not touch the host filesystem.
/// `..` above the root stays at `/`.
fn normalize_path(path: &Path) -> PathBuf {
    let mut segments: Vec<&str> = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                segments.pop();
            }
            Component::Normal(seg) => {
                if let Some(s) = seg.to_str() {
                    segments.push(s);
                }
            }
            Component::Prefix(_) => {}
        }
    }
    let mut result = PathBuf::from("/");
    for seg in segments {
        result.push(seg);
    }
    result
}

/// Check whether a slice of flag strings contains a flag that enables recursive
/// mode (e.g. `-r`, `-R`, `-a`, or combined short flags like `-rf`, `-Rf`).
///
/// Long-form flags (`--...`) are never treated as recursive — only short flags
/// (single dash) are scanned for the `r`/`R` character. This prevents false
/// positives on flags like `--preserve-root` or `--remove-destination`.
fn has_recursive_flag(flags: &[&str]) -> bool {
    flags.iter().any(|f| {
        if *f == "-a" {
            return true;
        }
        // Short flag cluster (e.g. `-rf`, `-Rf`): single dash, no second dash.
        f.starts_with('-') && !f.starts_with("--") && (f.contains('r') || f.contains('R'))
    })
}

/// Handle a `mkdir` sub-command.
///
/// Recognises the `-p` flag: with it, all ancestor directories are created;
/// without it, only the final component is created (parent must already exist,
/// otherwise an UnmodeledRunCommand warning is emitted and the path is skipped).
fn handle_mkdir(state: &mut PreviewState, argv: &[&str], sub_cmd: &str) -> SubCommandResult {
    // Separate flags from path arguments.
    // Only match short flags for `-p`; long-form flags like `--parents` are
    // not modeled, and scanning for 'p' in long flags would produce false positives.
    let has_p = argv
        .iter()
        .any(|a| *a == "-p" || (a.starts_with('-') && !a.starts_with("--") && a.contains('p')));
    let paths: Vec<&str> = argv
        .iter()
        .copied()
        .filter(|a| !a.starts_with('-'))
        .collect();

    let prov_src = ProvenanceSource::RunCommand {
        command: sub_cmd.to_string(),
    };
    let mut files_changed: Vec<PathBuf> = Vec::new();

    for raw_path in paths {
        let path = resolve_path(&state.cwd, raw_path);

        if has_p {
            // -p: create ancestors first, then the final directory.
            ensure_ancestors(&mut state.fs, &path, prov_src.clone());
            if !state.fs.contains(&path) {
                state.fs.insert(
                    path.clone(),
                    FsNode::Directory(DirNode {
                        provenance: Provenance::new(prov_src.clone()),
                        permissions: None,
                    }),
                );
                files_changed.push(path);
            }
        } else {
            // No -p: parent must already exist.
            let parent = path.parent().unwrap_or(Path::new("/"));
            if !state.fs.contains(parent) && parent != Path::new("/") {
                // Parent does not exist — emit warning and skip.
                state.warnings.push(Warning::UnmodeledRunCommand {
                    command: sub_cmd.to_string(),
                });
                continue;
            }
            if !state.fs.contains(&path) {
                state.fs.insert(
                    path.clone(),
                    FsNode::Directory(DirNode {
                        provenance: Provenance::new(prov_src.clone()),
                        permissions: None,
                    }),
                );
                files_changed.push(path);
            }
        }
    }

    SubCommandResult {
        files_changed,
        env_changed: vec![],
    }
}

/// Handle a `touch` sub-command.
///
/// Creates an empty file at each path argument. If the file already exists it
/// is left unchanged (no overwrite). Ancestor directories are created
/// automatically, mirroring `ensure_ancestors` behaviour.
fn handle_touch(state: &mut PreviewState, argv: &[&str], sub_cmd: &str) -> SubCommandResult {
    // Filter out leading option flags (e.g. -a, -m, -t ...).
    let paths: Vec<&str> = argv
        .iter()
        .copied()
        .filter(|a| !a.starts_with('-'))
        .collect();

    let prov_src = ProvenanceSource::RunCommand {
        command: sub_cmd.to_string(),
    };
    let mut files_changed: Vec<PathBuf> = Vec::new();

    for raw_path in paths {
        let path = resolve_path(&state.cwd, raw_path);

        // Ensure parent directories exist before inserting the file node.
        ensure_ancestors(&mut state.fs, &path, prov_src.clone());

        // Only create the file if it does not already exist.
        if !state.fs.contains(&path) {
            state.fs.insert(
                path.clone(),
                FsNode::File(FileNode {
                    content: vec![],
                    provenance: Provenance::new(prov_src.clone()),
                    permissions: None,
                }),
            );
            files_changed.push(path);
        }
    }

    SubCommandResult {
        files_changed,
        env_changed: vec![],
    }
}

/// Handle an `rm` sub-command.
///
/// Recognises `-r`/`-R` for recursive removal and `-f` for silent
/// "missing-is-ok" behaviour (which is the default anyway). Combined
/// short flags such as `-rf`, `-fr`, or `-Rf` are also recognised.
/// Missing paths are silently ignored.
fn handle_rm(state: &mut PreviewState, argv: &[&str], _sub_cmd: &str) -> SubCommandResult {
    let flags: Vec<&str> = argv
        .iter()
        .copied()
        .filter(|a| a.starts_with('-'))
        .collect();
    let paths: Vec<&str> = argv
        .iter()
        .copied()
        .filter(|a| !a.starts_with('-'))
        .collect();
    let recursive = has_recursive_flag(&flags);

    let mut files_changed: Vec<PathBuf> = Vec::new();

    for raw_path in paths {
        let path = resolve_path(&state.cwd, raw_path);

        // Guard: never recursively remove `/` — emit an unmodeled warning instead.
        // `rm -rf /` in a Dockerfile is either a multi-stage cleanup pattern or a
        // mistake; either way, wiping the entire virtual filesystem silently would
        // produce confusing results with no diagnostic.
        if recursive && path == Path::new("/") {
            state.warnings.push(Warning::UnmodeledRunCommand {
                command: format!("rm {} /", raw_path),
            });
            continue;
        }

        if recursive {
            let removed = state.fs.remove_recursive(&path);
            files_changed.extend(removed);
        } else if state.fs.remove(&path).is_some() {
            files_changed.push(path);
        }
    }

    SubCommandResult {
        files_changed,
        env_changed: vec![],
    }
}

/// Handle an `mv` sub-command.
///
/// Expects exactly two non-flag arguments (src, dst). If the source path does
/// not exist in the virtual filesystem an UnmodeledRunCommand warning is
/// emitted and nothing is changed.
fn handle_mv(state: &mut PreviewState, argv: &[&str], sub_cmd: &str) -> SubCommandResult {
    // Filter flags (-f, -n etc.) but do not model them.
    let args: Vec<&str> = argv
        .iter()
        .copied()
        .filter(|a| !a.starts_with('-'))
        .collect();
    if args.len() != 2 {
        state.warnings.push(Warning::UnmodeledRunCommand {
            command: sub_cmd.to_string(),
        });
        return SubCommandResult::empty();
    }

    let src = resolve_path(&state.cwd, args[0]);
    let dst = resolve_path(&state.cwd, args[1]);

    // Source must exist in the filesystem.
    if !state.fs.contains(&src) {
        state.warnings.push(Warning::UnmodeledRunCommand {
            command: sub_cmd.to_string(),
        });
        return SubCommandResult::empty();
    }

    // Collect the subtree rooted at src, remove it, then re-insert under dst.
    let subtree = state.fs.clone_subtree(&src);
    state.fs.remove_recursive(&src);

    let prov_src = ProvenanceSource::RunCommand {
        command: sub_cmd.to_string(),
    };
    let mut files_changed: Vec<PathBuf> = Vec::new();

    for (old_path, node) in subtree {
        // Remap the path: strip src prefix and join to dst.
        // clone_subtree guarantees all returned paths are src-prefixed, so
        // strip_prefix will succeed. If the invariant is somehow violated,
        // skip the entry rather than panicking so the simulation stays alive.
        let Ok(suffix) = old_path.strip_prefix(&src) else {
            continue;
        };
        let new_path = dst.join(suffix);

        // Ensure ancestor directories exist for deeply nested paths.
        ensure_ancestors(&mut state.fs, &new_path, prov_src.clone());

        // Clone the node and update its provenance to reflect the move.
        let new_node = match node {
            FsNode::File(f) => FsNode::File(FileNode {
                content: f.content,
                provenance: Provenance::new(prov_src.clone()),
                permissions: f.permissions,
            }),
            FsNode::Directory(_) => FsNode::Directory(DirNode {
                provenance: Provenance::new(prov_src.clone()),
                permissions: None,
            }),
            FsNode::Symlink(s) => FsNode::Symlink(crate::model::fs::SymlinkNode {
                target: s.target,
                provenance: Provenance::new(prov_src.clone()),
            }),
        };

        state.fs.insert(new_path.clone(), new_node);
        files_changed.push(new_path);
    }

    SubCommandResult {
        files_changed,
        env_changed: vec![],
    }
}

/// Handle a `cp` sub-command.
///
/// Recognises `-r`/`-R`/`-a` for recursive directory copies. For a single
/// file source, the file is cloned to dst. For a directory without a recursive
/// flag an UnmodeledRunCommand warning is emitted.
fn handle_cp(state: &mut PreviewState, argv: &[&str], sub_cmd: &str) -> SubCommandResult {
    let flags: Vec<&str> = argv
        .iter()
        .copied()
        .filter(|a| a.starts_with('-'))
        .collect();
    let args: Vec<&str> = argv
        .iter()
        .copied()
        .filter(|a| !a.starts_with('-'))
        .collect();
    let recursive = has_recursive_flag(&flags);

    if args.len() != 2 {
        state.warnings.push(Warning::UnmodeledRunCommand {
            command: sub_cmd.to_string(),
        });
        return SubCommandResult::empty();
    }

    let src = resolve_path(&state.cwd, args[0]);
    let dst = resolve_path(&state.cwd, args[1]);

    // Source must exist in the filesystem.
    if !state.fs.contains(&src) {
        state.warnings.push(Warning::UnmodeledRunCommand {
            command: sub_cmd.to_string(),
        });
        return SubCommandResult::empty();
    }

    let prov_src = ProvenanceSource::RunCommand {
        command: sub_cmd.to_string(),
    };
    let mut files_changed: Vec<PathBuf> = Vec::new();

    // Check if src is a directory.
    let src_is_dir = matches!(state.fs.get(&src), Some(FsNode::Directory(_)));

    if src_is_dir && !recursive {
        // Directory copy without -r is not supported.
        state.warnings.push(Warning::UnmodeledRunCommand {
            command: sub_cmd.to_string(),
        });
        return SubCommandResult::empty();
    }

    if src_is_dir {
        // Recursive directory copy: clone the entire subtree.
        let subtree = state.fs.clone_subtree(&src);

        for (old_path, node) in subtree {
            // clone_subtree guarantees all returned paths are src-prefixed; skip
            // any entry that violates the invariant rather than panicking.
            let Ok(suffix) = old_path.strip_prefix(&src) else {
                continue;
            };
            let new_path = dst.join(suffix);

            ensure_ancestors(&mut state.fs, &new_path, prov_src.clone());

            let new_node = match node {
                FsNode::File(f) => FsNode::File(FileNode {
                    content: f.content,
                    provenance: Provenance::new(prov_src.clone()),
                    permissions: f.permissions,
                }),
                FsNode::Directory(_) => FsNode::Directory(DirNode {
                    provenance: Provenance::new(prov_src.clone()),
                    permissions: None,
                }),
                FsNode::Symlink(s) => FsNode::Symlink(crate::model::fs::SymlinkNode {
                    target: s.target,
                    provenance: Provenance::new(prov_src.clone()),
                }),
            };

            state.fs.insert(new_path.clone(), new_node);
            files_changed.push(new_path);
        }
    } else {
        // Single file copy. The `contains` guard above ensures src exists, but
        // use `if let` instead of `expect` to stay clippy-clean.
        let Some(node) = state.fs.get(&src).cloned() else {
            return SubCommandResult::empty();
        };

        ensure_ancestors(&mut state.fs, &dst, prov_src.clone());

        let new_node = match node {
            FsNode::File(f) => FsNode::File(FileNode {
                content: f.content,
                provenance: Provenance::new(prov_src.clone()),
                permissions: f.permissions,
            }),
            other => other,
        };

        state.fs.insert(dst.clone(), new_node);
        files_changed.push(dst);
    }

    SubCommandResult {
        files_changed,
        env_changed: vec![],
    }
}

/// Split a raw shell command string into individual sub-commands.
///
/// Handles `&&`-chains and `;`-separated sequences. Each segment is trimmed
/// and empty segments are discarded.
///
/// **Limitation:** shell quoting is not modeled. A `&&` or `;` inside a
/// quoted string (e.g. `echo "a && b"`) will be treated as a delimiter.
/// This is an in-scope approximation for v0.1.
fn split_commands(raw: &str) -> Vec<&str> {
    // Two-pass split: first on `&&`, then on `;` within each segment.
    // All returned slices borrow from `raw` — no intermediate allocations.
    // `||` and other shell operators are intentionally not split and
    // pass through as part of a single sub-command token.
    raw.split("&&")
        .flat_map(|segment| segment.split(';'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Handle a package install sub-command for any supported package manager.
///
/// `manager` is the canonical registry key (e.g. "apt", "pip", "npm", "apk").
/// `argv` is everything after the binary name (e.g. for `apt-get install -y curl`,
/// argv would be `["install", "-y", "curl"]`).
/// `install_keywords` lists the sub-command tokens that trigger an install
/// (e.g. `&["install"]` for apt/pip/npm, `&["add"]` for apk).
///
/// Logic:
/// 1. Scan `argv` for the first token matching an install keyword. If none found,
///    the command is unmodeled (e.g. `apt-get update`).
/// 2. Collect tokens after the keyword, stripping any that start with `-`.
/// 3. If no packages remain after stripping, the command is unmodeled.
/// 4. Record each package in the registry and emit a `SimulatedInstall` warning.
fn handle_package_install(
    state: &mut PreviewState,
    manager: &str,
    argv: &[&str],
    sub_cmd: &str,
    install_keywords: &[&str],
) -> SubCommandResult {
    // Find the index of the first install keyword in argv.
    let keyword_pos = argv
        .iter()
        .position(|token| install_keywords.contains(token));

    let keyword_idx = match keyword_pos {
        Some(idx) => idx,
        None => {
            // No install keyword found — this is an unmodeled sub-command (e.g. apt-get update).
            state.warnings.push(Warning::UnmodeledRunCommand {
                command: sub_cmd.to_string(),
            });
            return SubCommandResult::empty();
        }
    };

    // Collect tokens after the keyword, filtering out flag tokens (those starting with '-').
    let packages: Vec<String> = argv[keyword_idx + 1..]
        .iter()
        .filter(|token| !token.starts_with('-'))
        .map(|token| token.to_string())
        .collect();

    if packages.is_empty() {
        // No package names after stripping flags — unmodeled (e.g. `apt-get install -y`).
        state.warnings.push(Warning::UnmodeledRunCommand {
            command: sub_cmd.to_string(),
        });
        return SubCommandResult::empty();
    }

    // Record each package in the installed registry under the canonical manager key.
    // `record()` returns false for unrecognised manager names — emit an UnmodeledRunCommand
    // so the user knows nothing was stored, rather than silently dropping the install.
    for pkg in &packages {
        if !state.installed.record(manager, pkg.clone()) {
            state.warnings.push(Warning::UnmodeledRunCommand {
                command: sub_cmd.to_string(),
            });
            return SubCommandResult::empty();
        }
    }

    // Sort packages alphabetically so the warning matches the order that
    // `InstalledRegistry::list()` will return them, giving a consistent user view.
    let mut sorted_packages = packages;
    sorted_packages.sort();

    // Emit a SimulatedInstall warning so the user knows the install was approximated.
    state.warnings.push(Warning::SimulatedInstall {
        manager: manager.to_string(),
        packages: sorted_packages,
    });

    SubCommandResult::empty()
}

/// Dispatch a single sub-command against the current simulation state.
///
/// Modeled commands (mkdir, touch, rm, mv, cp) are handled directly and
/// mutate the virtual filesystem. Package manager commands (apt-get, apt, pip,
/// pip3, npm, apk) record into the installed registry and emit SimulatedInstall
/// warnings. All other commands fall through to the `UnmodeledRunCommand` warning
/// path so the user can see what was skipped.
fn dispatch_sub_command(state: &mut PreviewState, sub_cmd: &str) -> SubCommandResult {
    let argv = parse_argv(sub_cmd);
    match argv.first().copied().unwrap_or("") {
        "mkdir" => handle_mkdir(state, &argv[1..], sub_cmd),
        "touch" => handle_touch(state, &argv[1..], sub_cmd),
        "rm" => handle_rm(state, &argv[1..], sub_cmd),
        "mv" => handle_mv(state, &argv[1..], sub_cmd),
        "cp" => handle_cp(state, &argv[1..], sub_cmd),
        // apt and apt-get share the "apt" registry key.
        "apt-get" | "apt" => {
            handle_package_install(state, "apt", &argv[1..], sub_cmd, &["install"])
        }
        // pip and pip3 share the "pip" registry key.
        "pip" | "pip3" => handle_package_install(state, "pip", &argv[1..], sub_cmd, &["install"]),
        // npm supports both "install" and its short alias "i".
        "npm" => handle_package_install(state, "npm", &argv[1..], sub_cmd, &["install", "i"]),
        // apk uses "add" as its install keyword.
        "apk" => handle_package_install(state, "apk", &argv[1..], sub_cmd, &["add"]),
        _ => {
            // Record the sub-command as unmodeled so the user sees it in warnings.
            state.warnings.push(Warning::UnmodeledRunCommand {
                command: sub_cmd.to_string(),
            });
            SubCommandResult::empty()
        }
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Handle a `RUN` instruction by splitting it into sub-commands and recording
/// each one as unmodeled.
///
/// `&&`-chains and `;`-separated commands are each dispatched individually so
/// the user gets a per-sub-command warning entry rather than a single opaque
/// blob. All resulting `files_changed` and `env_changed` entries are merged
/// into a single `LayerSummary` (matching Docker's one-layer-per-RUN model).
///
/// No real shell commands are executed. Every sub-command is preserved in
/// `state.warnings` so it appears in `:history`.
pub(crate) fn handle_run(state: &mut PreviewState, command: &str, _line: usize) -> LayerSummary {
    let sub_commands = split_commands(command);

    // Dispatch each sub-command and merge the results.
    let mut all_files: Vec<PathBuf> = Vec::new();
    let mut all_env: Vec<(String, String)> = Vec::new();

    for sub_cmd in sub_commands {
        let result = dispatch_sub_command(state, sub_cmd);
        all_files.extend(result.files_changed);
        all_env.extend(result.env_changed);
    }

    LayerSummary {
        instruction_type: "RUN".to_string(),
        files_changed: all_files,
        env_changed: all_env,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::model::{state::PreviewState, warning::Warning};

    // ── Pre-existing tests (must remain unchanged and green) ──────────────

    #[test]
    fn run_emits_unmodeled_warning() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "echo hello", 1);

        let has_warning = state.warnings.iter().any(
            |w| matches!(w, Warning::UnmodeledRunCommand { command } if command == "echo hello"),
        );
        assert!(has_warning, "expected UnmodeledRunCommand warning");
    }

    #[test]
    fn run_layer_summary_has_correct_type() {
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "make build", 1);
        assert_eq!(layer.instruction_type, "RUN");
    }

    #[test]
    fn run_layer_summary_has_no_files_changed() {
        // Use an unmodeled command (curl) so files_changed stays empty.
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "curl https://example.com", 1);
        assert!(
            layer.files_changed.is_empty(),
            "unmodeled RUN must not record file changes"
        );
    }

    #[test]
    fn package_install_does_not_mutate_fs_or_env() {
        // Package install commands record into InstalledRegistry but must never
        // write to the virtual filesystem or env map.
        let mut state = PreviewState::default();
        let fs_before = state.fs.iter().count();
        let env_before = state.env.len();

        handle_run(&mut state, "apt-get install -y curl", 1);

        assert_eq!(
            state.fs.iter().count(),
            fs_before,
            "fs must not change after package install"
        );
        assert_eq!(
            state.env.len(),
            env_before,
            "env must not change after package install"
        );
    }

    // ── split_commands unit tests ─────────────────────────────────────────

    #[test]
    fn split_single_command() {
        assert_eq!(split_commands("echo hello"), vec!["echo hello"]);
    }

    #[test]
    fn split_and_chain() {
        assert_eq!(
            split_commands("apt-get update && apt-get install -y curl"),
            vec!["apt-get update", "apt-get install -y curl"]
        );
    }

    #[test]
    fn split_semicolon_chain() {
        assert_eq!(split_commands("echo a; echo b"), vec!["echo a", "echo b"]);
    }

    #[test]
    fn split_mixed_delimiters() {
        assert_eq!(
            split_commands("echo a && echo b; echo c"),
            vec!["echo a", "echo b", "echo c"]
        );
    }

    #[test]
    fn split_trims_whitespace() {
        assert_eq!(
            split_commands("  echo a  &&  echo b  "),
            vec!["echo a", "echo b"]
        );
    }

    #[test]
    fn split_skips_empty_segments() {
        // Two consecutive `&&` produce an empty segment between them.
        assert_eq!(
            split_commands("echo a && && echo b"),
            vec!["echo a", "echo b"]
        );
    }

    #[test]
    fn split_only_delimiters() {
        // A string composed only of delimiters and whitespace yields no commands.
        assert_eq!(split_commands("&& ; &&"), Vec::<&str>::new());
    }

    #[test]
    fn split_empty_string() {
        assert_eq!(split_commands(""), Vec::<&str>::new());
    }

    #[test]
    fn split_trailing_and_chain() {
        // A trailing `&&` produces no extra empty element.
        assert_eq!(split_commands("echo a &&"), vec!["echo a"]);
    }

    #[test]
    fn split_does_not_split_on_or_operator() {
        // `||` is not a recognized delimiter and must be preserved intact.
        assert_eq!(split_commands("cmd1 || fallback"), vec!["cmd1 || fallback"]);
    }

    // ── handle_run integration tests ──────────────────────────────────────

    #[test]
    fn run_and_chain_emits_per_subcommand_warnings() {
        // After #21: apt-get update → UnmodeledRunCommand; apt-get install → SimulatedInstall.
        // Two warnings total: one per sub-command.
        let mut state = PreviewState::default();
        handle_run(&mut state, "apt-get update && apt-get install -y curl", 1);

        assert_eq!(
            state.warnings.len(),
            2,
            "expected one warning per sub-command, got {}",
            state.warnings.len()
        );

        // First warning: apt-get update is unmodeled (no install keyword).
        assert!(
            matches!(
                &state.warnings[0],
                Warning::UnmodeledRunCommand { command } if command == "apt-get update"
            ),
            "first warning should be UnmodeledRunCommand for 'apt-get update', got: {:?}",
            state.warnings[0]
        );

        // Second warning: apt-get install emits SimulatedInstall (not UnmodeledRunCommand).
        assert!(
            matches!(
                &state.warnings[1],
                Warning::SimulatedInstall { manager, .. } if manager == "apt"
            ),
            "second warning should be SimulatedInstall for apt-get install, got: {:?}",
            state.warnings[1]
        );
    }

    #[test]
    fn run_semicolon_chain_emits_per_subcommand_warnings() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "echo a; echo b; echo c", 1);

        assert_eq!(
            state.warnings.len(),
            3,
            "expected 3 warnings for 3 semicolon-separated sub-commands"
        );
        // Verify each warning carries the correct command text, not just the count.
        assert!(
            matches!(&state.warnings[0], Warning::UnmodeledRunCommand { command } if command == "echo a"),
            "first warning should be for 'echo a'"
        );
        assert!(
            matches!(&state.warnings[1], Warning::UnmodeledRunCommand { command } if command == "echo b"),
            "second warning should be for 'echo b'"
        );
        assert!(
            matches!(&state.warnings[2], Warning::UnmodeledRunCommand { command } if command == "echo c"),
            "third warning should be for 'echo c'"
        );
    }

    #[test]
    fn run_chain_returns_single_layer_summary() {
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "a && b && c", 1);

        assert_eq!(layer.instruction_type, "RUN");
        assert!(
            layer.files_changed.is_empty(),
            "unmodeled commands must not produce file changes"
        );
        assert!(
            layer.env_changed.is_empty(),
            "unmodeled commands must not produce env changes"
        );
    }

    #[test]
    fn run_or_operator_preserved_as_single_warning() {
        // `||` is not a delimiter — the entire token must appear in one warning.
        let mut state = PreviewState::default();
        handle_run(&mut state, "cmd1 || fallback", 1);

        assert_eq!(
            state.warnings.len(),
            1,
            "|| must not split into two warnings"
        );
        assert!(
            matches!(
                &state.warnings[0],
                Warning::UnmodeledRunCommand { command } if command == "cmd1 || fallback"
            ),
            "full '||' token must be preserved in the single warning"
        );
    }

    #[test]
    fn run_empty_command_emits_no_warnings() {
        // An empty RUN command string produces zero warnings and a valid LayerSummary.
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "", 1);

        assert!(
            state.warnings.is_empty(),
            "empty command must emit no warnings"
        );
        assert_eq!(layer.instruction_type, "RUN");
        assert!(layer.files_changed.is_empty());
        assert!(layer.env_changed.is_empty());
    }

    #[test]
    fn run_single_command_still_works() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "echo hello", 1);

        assert_eq!(
            state.warnings.len(),
            1,
            "single command should produce exactly one warning"
        );
        assert!(
            matches!(
                &state.warnings[0],
                Warning::UnmodeledRunCommand { command } if command == "echo hello"
            ),
            "warning command text must match the input"
        );
    }

    // ── parse_argv / resolve_path unit tests ──────────────────────────────

    #[test]
    fn parse_argv_splits_on_whitespace() {
        let result = parse_argv("mkdir -p /foo/bar");
        assert_eq!(result, vec!["mkdir", "-p", "/foo/bar"]);
    }

    #[test]
    fn parse_argv_empty_string_returns_empty() {
        let result = parse_argv("");
        assert!(result.is_empty());
    }

    #[test]
    fn resolve_path_absolute_returns_as_is() {
        let cwd = std::path::Path::new("/work");
        let result = resolve_path(cwd, "/etc/hosts");
        assert_eq!(result, PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn resolve_path_relative_joins_cwd() {
        let cwd = std::path::Path::new("/work");
        let result = resolve_path(cwd, "out/file.txt");
        assert_eq!(result, PathBuf::from("/work/out/file.txt"));
    }

    // ── mkdir tests ───────────────────────────────────────────────────────

    #[test]
    fn run_mkdir_creates_directory() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir /newdir", 1);
        assert!(
            state.fs.contains(std::path::Path::new("/newdir")),
            "mkdir must create /newdir"
        );
    }

    #[test]
    fn run_mkdir_p_creates_ancestors() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir -p /a/b/c", 1);
        assert!(state.fs.contains(std::path::Path::new("/a")));
        assert!(state.fs.contains(std::path::Path::new("/a/b")));
        assert!(state.fs.contains(std::path::Path::new("/a/b/c")));
    }

    #[test]
    fn run_mkdir_p_multiple_paths() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir -p /x/y /z/w", 1);
        assert!(state.fs.contains(std::path::Path::new("/x/y")));
        assert!(state.fs.contains(std::path::Path::new("/z/w")));
    }

    #[test]
    fn run_mkdir_sets_run_command_provenance() {
        use crate::model::provenance::ProvenanceSource;
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir /mydir", 1);
        let node = state
            .fs
            .get(std::path::Path::new("/mydir"))
            .expect("dir must exist");
        assert!(
            matches!(
                node.provenance().created_by,
                ProvenanceSource::RunCommand { .. }
            ),
            "provenance must be RunCommand"
        );
    }

    #[test]
    fn run_mkdir_does_not_emit_unmodeled_warning() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir /newdir", 1);
        let has_unmodeled = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. }));
        assert!(!has_unmodeled, "mkdir must not emit UnmodeledRunCommand");
    }

    #[test]
    fn run_mkdir_relative_path_resolves_against_cwd() {
        let mut state = PreviewState::default();
        state.cwd = PathBuf::from("/work");
        handle_run(&mut state, "mkdir -p subdir", 1);
        assert!(state.fs.contains(std::path::Path::new("/work/subdir")));
    }

    #[test]
    fn run_mkdir_existing_dir_is_noop() {
        // mkdir on an existing directory (with -p) must not fail or duplicate.
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir -p /app", 1);
        handle_run(&mut state, "mkdir -p /app", 1);
        // Still exactly one node at /app.
        assert!(state.fs.contains(std::path::Path::new("/app")));
    }

    // ── touch tests ───────────────────────────────────────────────────────

    #[test]
    fn run_touch_creates_empty_file() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "touch /tmp/x", 1);
        let node = state
            .fs
            .get(std::path::Path::new("/tmp/x"))
            .expect("/tmp/x must exist");
        match node {
            crate::model::fs::FsNode::File(f) => {
                assert!(f.content.is_empty(), "touch must create an empty file")
            }
            _ => panic!("expected File node"),
        }
    }

    #[test]
    fn run_touch_existing_file_is_noop() {
        use crate::model::fs::{FileNode, FsNode};
        use crate::model::provenance::{Provenance, ProvenanceSource};
        let mut state = PreviewState::default();
        // Pre-insert a file with known content.
        state.fs.insert(
            PathBuf::from("/existing.txt"),
            FsNode::File(FileNode {
                content: b"original".to_vec(),
                provenance: Provenance::new(ProvenanceSource::Workdir),
                permissions: None,
            }),
        );
        handle_run(&mut state, "touch /existing.txt", 1);
        // Content must be unchanged.
        let node = state
            .fs
            .get(std::path::Path::new("/existing.txt"))
            .expect("file must still exist");
        match node {
            FsNode::File(f) => assert_eq!(f.content, b"original"),
            _ => panic!("expected File node"),
        }
    }

    #[test]
    fn run_touch_creates_ancestors() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "touch /a/b/file.txt", 1);
        assert!(
            state.fs.contains(std::path::Path::new("/a")),
            "/a must be created as ancestor"
        );
        assert!(
            state.fs.contains(std::path::Path::new("/a/b")),
            "/a/b must be created as ancestor"
        );
        assert!(state.fs.contains(std::path::Path::new("/a/b/file.txt")));
    }

    #[test]
    fn run_touch_does_not_emit_unmodeled_warning() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "touch /tmp/x", 1);
        let has_unmodeled = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. }));
        assert!(!has_unmodeled, "touch must not emit UnmodeledRunCommand");
    }

    // ── rm tests ──────────────────────────────────────────────────────────

    #[test]
    fn run_rm_removes_file() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "touch /tmp/x", 1);
        assert!(state.fs.contains(std::path::Path::new("/tmp/x")));
        handle_run(&mut state, "rm /tmp/x", 1);
        assert!(!state.fs.contains(std::path::Path::new("/tmp/x")));
    }

    #[test]
    fn run_rm_absent_file_is_noop() {
        // rm on a non-existent file must not panic or emit a hard error.
        let mut state = PreviewState::default();
        handle_run(&mut state, "rm /nonexistent", 1);
        // No crash, no unmodeled warning for rm itself.
        let has_rm_warning = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { command } if command == "rm /nonexistent"));
        assert!(!has_rm_warning);
    }

    #[test]
    fn run_rm_rf_removes_directory_and_descendants() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir -p /app/src", 1);
        handle_run(&mut state, "touch /app/main.rs", 1);
        handle_run(&mut state, "touch /app/src/lib.rs", 1);

        handle_run(&mut state, "rm -rf /app", 1);

        assert!(!state.fs.contains(std::path::Path::new("/app")));
        assert!(!state.fs.contains(std::path::Path::new("/app/main.rs")));
        assert!(!state.fs.contains(std::path::Path::new("/app/src")));
        assert!(!state.fs.contains(std::path::Path::new("/app/src/lib.rs")));
    }

    #[test]
    fn run_rm_does_not_remove_string_prefix_sibling() {
        // rm -rf /app must not remove /appdata.
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir -p /app", 1);
        handle_run(&mut state, "mkdir -p /appdata", 1);

        handle_run(&mut state, "rm -rf /app", 1);

        assert!(!state.fs.contains(std::path::Path::new("/app")));
        assert!(
            state.fs.contains(std::path::Path::new("/appdata")),
            "/appdata must not be removed when removing /app"
        );
    }

    // ── mv tests ──────────────────────────────────────────────────────────

    #[test]
    fn run_mv_renames_file() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "touch /tmp/a.txt", 1);
        handle_run(&mut state, "mv /tmp/a.txt /tmp/b.txt", 1);
        assert!(!state.fs.contains(std::path::Path::new("/tmp/a.txt")));
        assert!(state.fs.contains(std::path::Path::new("/tmp/b.txt")));
    }

    #[test]
    fn run_mv_renames_directory_with_descendants() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir -p /old/sub", 1);
        handle_run(&mut state, "touch /old/file.txt", 1);
        handle_run(&mut state, "touch /old/sub/deep.txt", 1);

        handle_run(&mut state, "mv /old /new", 1);

        assert!(!state.fs.contains(std::path::Path::new("/old")));
        assert!(state.fs.contains(std::path::Path::new("/new")));
        assert!(state.fs.contains(std::path::Path::new("/new/file.txt")));
        assert!(state.fs.contains(std::path::Path::new("/new/sub")));
        assert!(state.fs.contains(std::path::Path::new("/new/sub/deep.txt")));
    }

    #[test]
    fn run_mv_missing_source_emits_warning() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "mv /nonexistent /dest", 1);
        let has_unmodeled = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. }));
        assert!(has_unmodeled, "mv with missing source must emit warning");
    }

    // ── cp tests ──────────────────────────────────────────────────────────

    #[test]
    fn run_cp_copies_file() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "touch /tmp/src.txt", 1);
        handle_run(&mut state, "cp /tmp/src.txt /tmp/dst.txt", 1);
        assert!(state.fs.contains(std::path::Path::new("/tmp/src.txt")));
        assert!(state.fs.contains(std::path::Path::new("/tmp/dst.txt")));
    }

    #[test]
    fn run_cp_r_copies_directory_tree() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir -p /src/sub", 1);
        handle_run(&mut state, "touch /src/file.txt", 1);
        handle_run(&mut state, "touch /src/sub/deep.txt", 1);

        handle_run(&mut state, "cp -r /src /dst", 1);

        // Source remains.
        assert!(state.fs.contains(std::path::Path::new("/src")));
        // Destination is populated.
        assert!(state.fs.contains(std::path::Path::new("/dst")));
        assert!(state.fs.contains(std::path::Path::new("/dst/file.txt")));
        assert!(state.fs.contains(std::path::Path::new("/dst/sub")));
        assert!(state.fs.contains(std::path::Path::new("/dst/sub/deep.txt")));
    }

    #[test]
    fn run_cp_no_r_on_directory_emits_warning() {
        use crate::model::fs::{DirNode, FsNode};
        use crate::model::provenance::{Provenance, ProvenanceSource};
        // Pre-insert the source directory directly so no prior warnings exist.
        let mut state = PreviewState::default();
        state.fs.insert(
            PathBuf::from("/srcdir"),
            FsNode::Directory(DirNode {
                provenance: Provenance::new(ProvenanceSource::Workdir),
                permissions: None,
            }),
        );
        handle_run(&mut state, "cp /srcdir /dstdir", 1);
        let has_unmodeled = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. }));
        assert!(
            has_unmodeled,
            "cp on directory without -r must emit UnmodeledRunCommand"
        );
    }

    #[test]
    fn run_cp_missing_source_emits_warning() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "cp /nonexistent /dst", 1);
        let has_unmodeled = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. }));
        assert!(has_unmodeled, "cp with missing source must emit warning");
    }

    // ── mkdir without -p, missing parent ──────────────────────────────────

    #[test]
    fn run_mkdir_without_p_emits_warning_when_parent_absent() {
        // `mkdir /missing/child` — parent `/missing` does not exist and no -p flag.
        // The handler must emit UnmodeledRunCommand and must NOT create the directory.
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir /missing/child", 1);
        assert!(
            !state.fs.contains(std::path::Path::new("/missing/child")),
            "directory must not be created when parent is absent and -p is omitted"
        );
        assert!(
            state
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. })),
            "mkdir without -p on absent parent must emit UnmodeledRunCommand"
        );
    }

    // ── &&-chain state propagation ─────────────────────────────────────────

    #[test]
    fn run_and_chain_state_propagates_between_subcommands() {
        // `mkdir -p /app && touch /app/main.rs`: the touch must see /app created
        // by the mkdir earlier in the same handle_run call.
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir -p /app && touch /app/main.rs", 1);
        assert!(
            state.fs.contains(std::path::Path::new("/app")),
            "/app must be created by mkdir"
        );
        assert!(
            state.fs.contains(std::path::Path::new("/app/main.rs")),
            "/app/main.rs must be created by touch, using /app created by mkdir"
        );
    }

    // ── mv wrong argument count ────────────────────────────────────────────

    #[test]
    fn run_mv_with_one_arg_emits_warning() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "mv /only-one", 1);
        assert!(
            state
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. })),
            "mv with one argument must emit UnmodeledRunCommand"
        );
    }

    #[test]
    fn run_mv_with_three_args_emits_warning_and_leaves_fs_unchanged() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "touch /a && touch /b", 1);
        let node_count_before = state.fs.iter().count();
        handle_run(&mut state, "mv /a /b /c", 1);
        // fs must not change — mv with 3 args is unmodeled
        assert_eq!(
            state.fs.iter().count(),
            node_count_before,
            "mv with three args must not mutate the filesystem"
        );
        assert!(
            state
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. })),
            "mv with three args must emit UnmodeledRunCommand"
        );
    }

    // ── rm -rf / root guard ────────────────────────────────────────────────

    #[test]
    fn run_rm_rf_root_emits_warning_and_does_not_wipe_fs() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "mkdir -p /app && touch /app/main.rs", 1);
        let node_count_before = state.fs.iter().count();
        assert!(node_count_before > 0);

        handle_run(&mut state, "rm -rf /", 1);

        assert_eq!(
            state.fs.iter().count(),
            node_count_before,
            "rm -rf / must not remove any nodes from the virtual filesystem"
        );
        assert!(
            state
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. })),
            "rm -rf / must emit UnmodeledRunCommand"
        );
    }

    // ── has_recursive_flag unit tests ──────────────────────────────────────

    #[test]
    fn recursive_flag_detects_lowercase_r() {
        assert!(has_recursive_flag(&["-r"]));
    }

    #[test]
    fn recursive_flag_detects_uppercase_r() {
        assert!(has_recursive_flag(&["-R"]));
    }

    #[test]
    fn recursive_flag_detects_combined_rf() {
        assert!(has_recursive_flag(&["-rf"]));
    }

    #[test]
    fn recursive_flag_detects_combined_rf_upper() {
        assert!(has_recursive_flag(&["-Rf"]));
    }

    #[test]
    fn recursive_flag_detects_a_flag() {
        assert!(has_recursive_flag(&["-a"]));
    }

    #[test]
    fn recursive_flag_does_not_match_long_form() {
        // `--recursive` is a long-form flag and must NOT be treated as recursive
        // by the short-flag character scan.
        assert!(!has_recursive_flag(&["--recursive"]));
    }

    #[test]
    fn recursive_flag_does_not_match_preserve_root() {
        // `--preserve-root` contains 'r' but is a long-form flag — must not trigger.
        assert!(!has_recursive_flag(&["--preserve-root"]));
    }

    // ── normalize_path / resolve_path with .. ─────────────────────────────

    #[test]
    fn resolve_path_normalizes_dotdot() {
        // `/app/../etc` should normalize to `/etc`.
        let result = resolve_path(std::path::Path::new("/"), "/app/../etc");
        assert_eq!(result, PathBuf::from("/etc"));
    }

    #[test]
    fn resolve_path_clamps_dotdot_at_root() {
        // `/../..` must clamp at `/`.
        let result = resolve_path(std::path::Path::new("/"), "/../..");
        assert_eq!(result, PathBuf::from("/"));
    }

    // ── files_changed integration tests ───────────────────────────────────

    #[test]
    fn run_files_changed_populated_after_mkdir() {
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "mkdir -p /newdir", 1);
        assert!(
            !layer.files_changed.is_empty(),
            "mkdir must populate files_changed in the LayerSummary"
        );
        assert!(layer.files_changed.contains(&PathBuf::from("/newdir")));
    }

    #[test]
    fn run_files_changed_populated_after_touch() {
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "touch /newfile.txt", 1);
        assert!(
            !layer.files_changed.is_empty(),
            "touch must populate files_changed in the LayerSummary"
        );
        assert!(layer.files_changed.contains(&PathBuf::from("/newfile.txt")));
    }

    // ── Package install handler tests (#21) ───────────────────────────────

    #[test]
    fn apt_get_install_records_packages() {
        // apt-get install -y curl wget git → installed.list("apt") == ["curl", "git", "wget"]
        // BTreeSet sorts alphabetically.
        let mut state = PreviewState::default();
        handle_run(&mut state, "apt-get install -y curl wget git", 1);
        assert_eq!(
            state.installed.list("apt"),
            vec!["curl", "git", "wget"],
            "apt-get install must record all three packages in sorted order"
        );
    }

    #[test]
    fn apt_get_install_emits_simulated_install_warning() {
        // SimulatedInstall must list all three packages (sorted alphabetically).
        let mut state = PreviewState::default();
        handle_run(&mut state, "apt-get install -y curl wget git", 1);
        let simulated = state.warnings.iter().find(|w| {
            matches!(
                w,
                Warning::SimulatedInstall { manager, .. } if manager == "apt"
            )
        });
        assert!(
            simulated.is_some(),
            "expected a SimulatedInstall warning with manager 'apt'"
        );
        if let Some(Warning::SimulatedInstall { packages, .. }) = simulated {
            assert_eq!(
                packages,
                &vec!["curl".to_string(), "git".to_string(), "wget".to_string()],
                "SimulatedInstall packages must be sorted and contain all three packages"
            );
        }
    }

    #[test]
    fn apt_get_install_no_unmodeled_warning() {
        // apt-get install must NOT emit an UnmodeledRunCommand warning.
        let mut state = PreviewState::default();
        handle_run(&mut state, "apt-get install -y curl wget git", 1);
        let has_unmodeled = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. }));
        assert!(
            !has_unmodeled,
            "apt-get install must not emit UnmodeledRunCommand"
        );
    }

    #[test]
    fn apt_get_update_emits_unmodeled_run_command() {
        // "apt-get update" has no install keyword → must fall through to UnmodeledRunCommand.
        let mut state = PreviewState::default();
        handle_run(&mut state, "apt-get update", 1);
        let has_unmodeled = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. }));
        assert!(
            has_unmodeled,
            "apt-get update must emit UnmodeledRunCommand (no install keyword)"
        );
    }

    #[test]
    fn apt_install_alias_records_packages() {
        // "apt" (without "-get") must also dispatch to handle_package_install.
        let mut state = PreviewState::default();
        handle_run(&mut state, "apt install -y vim", 1);
        assert!(
            state.installed.list("apt").contains(&"vim".to_string()),
            "apt install must record 'vim' under the 'apt' manager"
        );
    }

    #[test]
    fn pip_install_records_packages() {
        // pip install requests flask → installed.list("pip") == ["flask", "requests"]
        let mut state = PreviewState::default();
        handle_run(&mut state, "pip install requests flask", 1);
        assert_eq!(
            state.installed.list("pip"),
            vec!["flask", "requests"],
            "pip install must record packages in sorted order"
        );
    }

    #[test]
    fn pip_install_strips_flags() {
        // Tokens starting with '-' must be excluded from the package list.
        let mut state = PreviewState::default();
        handle_run(&mut state, "pip install --no-cache-dir numpy", 1);
        assert_eq!(
            state.installed.list("pip"),
            vec!["numpy"],
            "pip install must record only 'numpy', not the flag"
        );
    }

    #[test]
    fn pip3_alias_records_packages() {
        // pip3 must use the same "pip" manager key as pip.
        let mut state = PreviewState::default();
        handle_run(&mut state, "pip3 install django", 1);
        assert!(
            state.installed.list("pip").contains(&"django".to_string()),
            "pip3 install must record 'django' under the 'pip' manager"
        );
    }

    #[test]
    fn pip_install_emits_simulated_install_warning() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "pip install requests", 1);
        let simulated = state.warnings.iter().find(|w| {
            matches!(
                w,
                Warning::SimulatedInstall { manager, .. } if manager == "pip"
            )
        });
        assert!(
            simulated.is_some(),
            "pip install must emit SimulatedInstall warning with manager 'pip'"
        );
        if let Some(Warning::SimulatedInstall { packages, .. }) = simulated {
            assert_eq!(
                packages,
                &vec!["requests".to_string()],
                "SimulatedInstall packages must contain 'requests'"
            );
        }
    }

    #[test]
    fn npm_install_records_packages() {
        // npm install -g typescript → installed.list("npm") contains "typescript"
        let mut state = PreviewState::default();
        handle_run(&mut state, "npm install -g typescript", 1);
        assert!(
            state
                .installed
                .list("npm")
                .contains(&"typescript".to_string()),
            "npm install must record 'typescript' under the 'npm' manager"
        );
    }

    #[test]
    fn npm_i_alias_records_packages() {
        // "npm i" (short alias for install) must be recognized.
        let mut state = PreviewState::default();
        handle_run(&mut state, "npm i express", 1);
        assert!(
            state.installed.list("npm").contains(&"express".to_string()),
            "npm i must record 'express' under the 'npm' manager"
        );
    }

    #[test]
    fn npm_install_emits_simulated_install_warning() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "npm install lodash", 1);
        let simulated = state.warnings.iter().find(|w| {
            matches!(
                w,
                Warning::SimulatedInstall { manager, .. } if manager == "npm"
            )
        });
        assert!(
            simulated.is_some(),
            "npm install must emit SimulatedInstall warning with manager 'npm'"
        );
        if let Some(Warning::SimulatedInstall { packages, .. }) = simulated {
            assert_eq!(
                packages,
                &vec!["lodash".to_string()],
                "SimulatedInstall packages must contain 'lodash'"
            );
        }
    }

    #[test]
    fn apk_add_records_packages() {
        // apk add --no-cache bash curl → installed.list("apk") == ["bash", "curl"]
        let mut state = PreviewState::default();
        handle_run(&mut state, "apk add --no-cache bash curl", 1);
        assert_eq!(
            state.installed.list("apk"),
            vec!["bash", "curl"],
            "apk add must record packages in sorted order"
        );
    }

    #[test]
    fn apk_add_emits_simulated_install_warning() {
        let mut state = PreviewState::default();
        handle_run(&mut state, "apk add git", 1);
        let simulated = state.warnings.iter().find(|w| {
            matches!(
                w,
                Warning::SimulatedInstall { manager, .. } if manager == "apk"
            )
        });
        assert!(
            simulated.is_some(),
            "apk add must emit SimulatedInstall warning with manager 'apk'"
        );
        if let Some(Warning::SimulatedInstall { packages, .. }) = simulated {
            assert_eq!(
                packages,
                &vec!["git".to_string()],
                "SimulatedInstall packages must contain 'git'"
            );
        }
    }

    #[test]
    fn install_command_returns_no_filesystem_changes() {
        // Package installs must not produce any files_changed in the LayerSummary.
        let mut state = PreviewState::default();
        let layer = handle_run(&mut state, "apt-get install -y curl", 1);
        assert!(
            layer.files_changed.is_empty(),
            "apt-get install must not produce filesystem changes"
        );
    }

    #[test]
    fn apt_get_update_then_install_chain_records_packages() {
        // Full realistic chain: update then install.
        // update emits UnmodeledRunCommand; install records packages.
        let mut state = PreviewState::default();
        handle_run(
            &mut state,
            "apt-get update && apt-get install -y curl wget git",
            1,
        );
        assert_eq!(
            state.installed.list("apt"),
            vec!["curl", "git", "wget"],
            "install after update must record all packages in sorted order"
        );
    }

    #[test]
    fn install_and_mkdir_chain_both_take_effect() {
        // Mixed chain: package install + filesystem command.
        // Both effects must be visible after a single handle_run call.
        let mut state = PreviewState::default();
        handle_run(&mut state, "apt-get install -y curl && mkdir -p /app", 1);
        assert!(
            state.installed.list("apt").contains(&"curl".to_string()),
            "curl must be recorded in the apt registry"
        );
        assert!(
            state.fs.contains(std::path::Path::new("/app")),
            "/app must be created by mkdir in the same chain"
        );
    }

    #[test]
    fn apt_get_install_flags_only_emits_unmodeled_run_command() {
        // "apt-get install -y" has the install keyword but no package names after
        // flag-stripping → falls through to UnmodeledRunCommand and records nothing.
        let mut state = PreviewState::default();
        handle_run(&mut state, "apt-get install -y", 1);
        assert!(
            state.installed.list("apt").is_empty(),
            "no packages must be recorded when only flags are present"
        );
        let has_unmodeled = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::UnmodeledRunCommand { .. }));
        assert!(
            has_unmodeled,
            "apt-get install with flags only must emit UnmodeledRunCommand"
        );
        let has_simulated = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::SimulatedInstall { .. }));
        assert!(
            !has_simulated,
            "apt-get install with flags only must not emit SimulatedInstall"
        );
    }

    #[test]
    fn semicolon_chain_install_records_packages() {
        // `;`-separated chains must work the same as `&&`-separated chains.
        let mut state = PreviewState::default();
        handle_run(&mut state, "apt-get update ; apt-get install -y curl", 1);
        assert_eq!(
            state.installed.list("apt"),
            vec!["curl"],
            "install after semicolon-separated update must record packages"
        );
    }
}
