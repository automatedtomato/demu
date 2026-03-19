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
/// rm, mv, cp). `env_changed` is reserved for future env-mutating commands.
/// Issue #21 will populate env_changed for `export`/`ENV`-style sub-commands.
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
        let suffix = old_path
            .strip_prefix(&src)
            .expect("clone_subtree always returns src-prefixed paths");
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
            let suffix = old_path
                .strip_prefix(&src)
                .expect("clone_subtree always returns src-prefixed paths");
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
        // Single file copy.
        let node = state
            .fs
            .get(&src)
            .expect("contains guard ensures src exists")
            .clone();

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

/// Dispatch a single sub-command against the current simulation state.
///
/// Modeled commands (mkdir, touch, rm, mv, cp) are handled directly and
/// mutate the virtual filesystem. All other commands fall through to the
/// `UnmodeledRunCommand` warning path so the user can see what was skipped.
fn dispatch_sub_command(state: &mut PreviewState, sub_cmd: &str) -> SubCommandResult {
    let argv = parse_argv(sub_cmd);
    match argv.first().copied().unwrap_or("") {
        "mkdir" => handle_mkdir(state, &argv[1..], sub_cmd),
        "touch" => handle_touch(state, &argv[1..], sub_cmd),
        "rm" => handle_rm(state, &argv[1..], sub_cmd),
        "mv" => handle_mv(state, &argv[1..], sub_cmd),
        "cp" => handle_cp(state, &argv[1..], sub_cmd),
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
    fn run_does_not_mutate_fs_or_env() {
        let mut state = PreviewState::default();
        let fs_before = state.fs.iter().count();
        let env_before = state.env.len();

        handle_run(&mut state, "apt-get install -y curl", 1);

        assert_eq!(
            state.fs.iter().count(),
            fs_before,
            "fs must not change after RUN stub"
        );
        assert_eq!(
            state.env.len(),
            env_before,
            "env must not change after RUN stub"
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
        let mut state = PreviewState::default();
        handle_run(&mut state, "apt-get update && apt-get install -y curl", 1);

        assert_eq!(
            state.warnings.len(),
            2,
            "expected one warning per sub-command, got {}",
            state.warnings.len()
        );

        // First warning must reference the first sub-command.
        assert!(
            matches!(
                &state.warnings[0],
                Warning::UnmodeledRunCommand { command } if command == "apt-get update"
            ),
            "first warning should be for 'apt-get update', got: {:?}",
            state.warnings[0]
        );

        // Second warning must reference the second sub-command.
        assert!(
            matches!(
                &state.warnings[1],
                Warning::UnmodeledRunCommand { command } if command == "apt-get install -y curl"
            ),
            "second warning should be for 'apt-get install -y curl', got: {:?}",
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
}
