// COPY instruction handler for the simulation engine.
//
// This module applies `Instruction::Copy` to a `PreviewState`. It reads real
// files from the host build context directory and inserts them as `FsNode`
// entries in the virtual filesystem, recording provenance for each node.
//
// Missing sources are surfaced as `Warning::MissingCopySource` rather than
// hard errors — the engine continues with an empty placeholder file.

use std::path::{Path, PathBuf};

use crate::model::{
    fs::{DirNode, FileNode, FsNode, VirtualFs},
    instruction::CopySource,
    provenance::{Provenance, ProvenanceSource},
    state::{LayerSummary, PreviewState, StageRegistry},
    warning::Warning,
};

use super::EngineError;

/// Ensure all ancestor directories of `path` exist in the virtual filesystem.
///
/// For each parent component of `path` (from root down), if no node already
/// exists at that path, a `FsNode::Directory` is inserted with the supplied
/// `provenance_source`.
///
/// This is called before inserting any file node so that `list_dir` and
/// `contains` queries on parent directories work correctly.
pub(crate) fn ensure_ancestors(
    fs: &mut VirtualFs,
    path: &Path,
    provenance_source: ProvenanceSource,
) {
    // Caller must supply an absolute path — relative paths would silently
    // produce wrong ancestor chains. This fires in debug/test builds.
    debug_assert!(
        path.is_absolute(),
        "ensure_ancestors: path must be absolute, got {path:?}"
    );

    // Collect all ancestor paths, from root to the immediate parent.
    let mut ancestors: Vec<PathBuf> = path.ancestors().skip(1).map(Path::to_path_buf).collect();
    // ancestors() yields longest-first; we want to insert root-first.
    ancestors.reverse();

    for ancestor in ancestors {
        // Skip the root component "/" — it is implicit, not stored.
        if ancestor == Path::new("/") {
            continue;
        }
        // Only insert if this directory is not already present.
        if !fs.contains(&ancestor) {
            fs.insert(
                ancestor,
                FsNode::Directory(DirNode {
                    provenance: Provenance::new(provenance_source.clone()),
                    permissions: None,
                }),
            );
        }
    }
}

/// Apply a `COPY` instruction to the simulation state.
///
/// # Parameters
/// - `state`       — mutable preview state to update
/// - `source`      — where to read source files from (host path or stage name)
/// - `dest`        — destination path inside the container
/// - `context_dir` — the host build context directory root
/// - `line`        — source line number for provenance tracking
/// - `stages`      — registry of completed build stages for `COPY --from=<stage>`
///
/// # Returns
/// A `LayerSummary` listing every path that was inserted into the filesystem.
/// Returns `Err(EngineError::Io)` only for unexpected I/O errors; "file not
/// found" is handled as a `Warning::MissingCopySource`.
pub(crate) fn handle_copy(
    state: &mut PreviewState,
    source: &CopySource,
    dest: &Path,
    context_dir: &Path,
    line: usize,
    stages: &StageRegistry,
) -> Result<LayerSummary, EngineError> {
    // Resolve the destination path: relative dests are joined to the current cwd.
    let dest_str = dest.to_string_lossy();
    let has_trailing_slash = dest_str.ends_with('/');

    // Normalise dest: absolute paths stay as-is; relative ones join with cwd.
    let resolved_dest = if dest.is_absolute() {
        dest.to_path_buf()
    } else {
        state.cwd.join(dest)
    };

    let mut files_changed: Vec<PathBuf> = Vec::new();

    match source {
        CopySource::Host(src_rel) => {
            let host_path = context_dir.join(src_rel);

            // Use metadata to determine if source exists and what kind it is.
            let meta = match std::fs::metadata(&host_path) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // Missing source is a warning, not a hard error.
                    state.warnings.push(Warning::MissingCopySource {
                        path: host_path.clone(),
                    });
                    // Insert an empty placeholder file at the destination.
                    // If the source path has no filename component (e.g. `..`),
                    // skip the placeholder — we cannot determine a valid dest.
                    let Some(final_dest) =
                        resolve_file_dest(&resolved_dest, src_rel, has_trailing_slash)
                    else {
                        return Ok(LayerSummary {
                            instruction_type: "COPY".to_string(),
                            files_changed,
                            env_changed: vec![],
                        });
                    };
                    let provenance = ProvenanceSource::CopyFromHost {
                        host_path: host_path.clone(),
                    };
                    ensure_ancestors(&mut state.fs, &final_dest, provenance.clone());
                    state.fs.insert(
                        final_dest.clone(),
                        FsNode::File(FileNode {
                            content: vec![],
                            provenance: Provenance::new(provenance),
                            permissions: None,
                        }),
                    );
                    files_changed.push(final_dest);
                    return Ok(LayerSummary {
                        instruction_type: "COPY".to_string(),
                        files_changed,
                        env_changed: vec![],
                    });
                }
                Err(e) => {
                    return Err(EngineError::Io {
                        path: host_path,
                        source: e,
                    });
                }
            };

            if meta.is_file() {
                // Single-file copy: resolve the final destination path.
                // If the source path has no filename component, skip silently
                // rather than writing to a bogus location.
                let Some(final_dest) =
                    resolve_file_dest(&resolved_dest, src_rel, has_trailing_slash)
                else {
                    return Ok(LayerSummary {
                        instruction_type: "COPY".to_string(),
                        files_changed,
                        env_changed: vec![],
                    });
                };
                let provenance = ProvenanceSource::CopyFromHost {
                    host_path: host_path.clone(),
                };
                let content = std::fs::read(&host_path).map_err(|e| EngineError::Io {
                    path: host_path.clone(),
                    source: e,
                })?;
                ensure_ancestors(&mut state.fs, &final_dest, provenance.clone());
                state.fs.insert(
                    final_dest.clone(),
                    FsNode::File(FileNode {
                        content,
                        provenance: Provenance::new(provenance),
                        permissions: None,
                    }),
                );
                files_changed.push(final_dest);
            } else if meta.is_dir() {
                // Directory copy: walk the tree and insert each file.
                // TODO: `has_trailing_slash` is not used for directory sources.
                // Docker's actual behavior: `COPY src/ /dest/` and `COPY src /dest`
                // both copy the contents of src into dest. Modeling this distinction
                // would require detecting whether the dest already exists; deferred
                // to a future iteration (see VirtualFs::contains + trailing-slash
                // handling in resolve_file_dest for the single-file analogue).
                copy_dir_recursive(
                    &host_path,
                    &host_path,
                    &resolved_dest,
                    &mut state.fs,
                    &mut files_changed,
                )?;
            }
        }
        // Stage-to-stage copy: look up the source stage in the registry,
        // then copy the specified path (file or directory) into this stage's fs.
        CopySource::Stage { name, src_path } => {
            // 1. Resolve the source stage. Relative src_paths are treated as
            //    absolute from the stage root (prepend "/" if not already absolute).
            let resolved_src = if src_path.is_absolute() {
                src_path.clone()
            } else {
                PathBuf::from("/").join(src_path)
            };

            // 2. Look up the stage by name or numeric index.
            let source_stage = match stages.get(name) {
                Some(s) => s,
                None => {
                    // Stage not found — emit MissingCopyStage warning and return empty.
                    state.warnings.push(Warning::MissingCopyStage {
                        stage: name.clone(),
                        line,
                    });
                    return Ok(LayerSummary {
                        instruction_type: "COPY".to_string(),
                        files_changed,
                        env_changed: vec![],
                    });
                }
            };

            // 3. Look up the source path in the stage's virtual filesystem.
            match source_stage.fs.get(&resolved_src) {
                None => {
                    // Path not found in source stage — emit MissingCopySource warning.
                    state.warnings.push(Warning::MissingCopySource {
                        path: resolved_src.clone(),
                    });
                    // Insert an empty placeholder file at the destination so the
                    // user can still see something at the expected path.
                    let provenance = ProvenanceSource::CopyFromStage {
                        stage: name.clone(),
                    };
                    ensure_ancestors(&mut state.fs, &resolved_dest, provenance.clone());
                    state.fs.insert(
                        resolved_dest.clone(),
                        FsNode::File(FileNode {
                            content: vec![],
                            provenance: Provenance::new(provenance),
                            permissions: None,
                        }),
                    );
                    // Record in files_changed so the placeholder is visible in
                    // layer summaries (consistent with host-copy missing-source path).
                    files_changed.push(resolved_dest);
                }

                Some(FsNode::File(src_file)) => {
                    // Single-file copy from the source stage.
                    let provenance = ProvenanceSource::CopyFromStage {
                        stage: name.clone(),
                    };
                    // Determine the final destination (respects trailing slash).
                    let final_dest = resolve_file_dest(
                        &resolved_dest,
                        &resolved_src,
                        has_trailing_slash,
                    )
                    .unwrap_or(resolved_dest.clone());

                    ensure_ancestors(&mut state.fs, &final_dest, provenance.clone());
                    // Clone the source node content and replace provenance to record
                    // that this file originates from a stage copy, not the host.
                    state.fs.insert(
                        final_dest.clone(),
                        FsNode::File(FileNode {
                            content: src_file.content.clone(),
                            provenance: Provenance::new(provenance),
                            permissions: src_file.permissions,
                        }),
                    );
                    files_changed.push(final_dest);
                }

                Some(FsNode::Directory(_)) => {
                    // Directory copy: collect all nodes under resolved_src in the
                    // source stage's fs and insert them into this stage's fs.
                    //
                    // We clone the subtree to avoid borrowing source_stage while
                    // we mutably borrow state below.
                    let subtree = source_stage.fs.clone_subtree(&resolved_src);

                    for (abs_path, node) in subtree {
                        // Compute the relative path within the source directory.
                        let relative = match abs_path.strip_prefix(&resolved_src) {
                            Ok(r) => r.to_path_buf(),
                            // strip_prefix should never fail because clone_subtree
                            // only returns paths that start with resolved_src.
                            Err(_) => continue,
                        };

                        // The destination is resolved_dest joined with the relative suffix.
                        let resolved_dest_entry = if relative.as_os_str().is_empty() {
                            // The root of the subtree maps to resolved_dest itself.
                            resolved_dest.clone()
                        } else {
                            resolved_dest.join(&relative)
                        };

                        let provenance = ProvenanceSource::CopyFromStage {
                            stage: name.clone(),
                        };

                        ensure_ancestors(&mut state.fs, &resolved_dest_entry, provenance.clone());

                        match &node {
                            FsNode::File(src_file) => {
                                state.fs.insert(
                                    resolved_dest_entry.clone(),
                                    FsNode::File(FileNode {
                                        content: src_file.content.clone(),
                                        provenance: Provenance::new(provenance),
                                        permissions: src_file.permissions,
                                    }),
                                );
                                files_changed.push(resolved_dest_entry);
                            }
                            FsNode::Directory(src_dir) => {
                                if !state.fs.contains(&resolved_dest_entry) {
                                    state.fs.insert(
                                        resolved_dest_entry.clone(),
                                        FsNode::Directory(DirNode {
                                            provenance: Provenance::new(provenance),
                                            permissions: src_dir.permissions,
                                        }),
                                    );
                                }
                                files_changed.push(resolved_dest_entry);
                            }
                            FsNode::Symlink(_) => {
                                // Symlinks are not yet modeled for stage copies.
                                // Emit MissingCopySource so the user sees a diagnostic
                                // rather than a silent drop — consistent with the project
                                // rule: do not silently fake behaviour.
                                state.warnings.push(Warning::MissingCopySource {
                                    path: abs_path.clone(),
                                });
                            }
                        }
                    }
                }

                Some(FsNode::Symlink(_)) => {
                    // Symlinks are not modeled for stage copies yet.
                    // Emit MissingCopySource so the user sees a diagnostic.
                    state.warnings.push(Warning::MissingCopySource {
                        path: resolved_src.clone(),
                    });
                }
            }
        }
    }

    Ok(LayerSummary {
        instruction_type: "COPY".to_string(),
        files_changed,
        env_changed: vec![],
    })
}

/// Determine the final destination path for a single-file COPY.
///
/// If the destination has a trailing slash (was specified as a directory),
/// the source filename is appended. Otherwise the destination is used as-is.
///
/// Returns `None` when the source path has no file name component (e.g. `..`
/// or an empty path), which the caller must treat as an error or warning.
fn resolve_file_dest(dest: &Path, src_rel: &Path, had_trailing_slash: bool) -> Option<PathBuf> {
    if had_trailing_slash {
        // Dest is explicitly a directory: append just the filename component.
        // `file_name()` returns None for paths like `..` or `/`; bail out
        // rather than substituting a bogus OS string.
        let filename = src_rel.file_name()?;
        Some(dest.join(filename))
    } else {
        Some(dest.to_path_buf())
    }
}

/// Recursively copy all files under `src_root` into `dest_root` in the
/// virtual filesystem.
///
/// For each file found, the path relative to `src_root` is computed and
/// joined onto `dest_root`. Directory nodes are inserted for each directory
/// encountered.
fn copy_dir_recursive(
    src_root: &Path,
    current: &Path,
    dest_root: &Path,
    fs: &mut VirtualFs,
    files_changed: &mut Vec<PathBuf>,
) -> Result<(), EngineError> {
    let entries = std::fs::read_dir(current).map_err(|e| EngineError::Io {
        path: current.to_path_buf(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| EngineError::Io {
            path: current.to_path_buf(),
            source: e,
        })?;
        let entry_path = entry.path();
        let meta = std::fs::metadata(&entry_path).map_err(|e| EngineError::Io {
            path: entry_path.clone(),
            source: e,
        })?;

        // Compute the path suffix relative to the source root directory.
        // strip_prefix must succeed here because `entry_path` is always a
        // descendant of `src_root` (produced by read_dir walking src_root).
        // If it somehow fails, propagate an Io error rather than silently
        // substituting the host path into the virtual filesystem.
        let rel = entry_path
            .strip_prefix(src_root)
            .map_err(|_| EngineError::Io {
                path: entry_path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("entry {entry_path:?} is not under src_root {src_root:?}"),
                ),
            })?;
        let dest_path = dest_root.join(rel);

        let provenance = ProvenanceSource::CopyFromHost {
            host_path: entry_path.clone(),
        };

        if meta.is_dir() {
            // Ensure the directory node exists in the virtual fs.
            ensure_ancestors(fs, &dest_path, provenance.clone());
            if !fs.contains(&dest_path) {
                fs.insert(
                    dest_path.clone(),
                    FsNode::Directory(DirNode {
                        provenance: Provenance::new(provenance),
                        permissions: None,
                    }),
                );
            }
            files_changed.push(dest_path.clone());
            // Recurse into the subdirectory.
            copy_dir_recursive(src_root, &entry_path, dest_root, fs, files_changed)?;
        } else if meta.is_file() {
            let content = std::fs::read(&entry_path).map_err(|e| EngineError::Io {
                path: entry_path.clone(),
                source: e,
            })?;
            ensure_ancestors(fs, &dest_path, provenance.clone());
            fs.insert(
                dest_path.clone(),
                FsNode::File(FileNode {
                    content,
                    provenance: Provenance::new(provenance),
                    permissions: None,
                }),
            );
            files_changed.push(dest_path);
        }
    }

    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    // ── helpers ───────────────────────────────────────────────────────────

    /// Create a temp dir with a single file at `rel_path` containing `content`.
    fn make_context_with_file(rel_path: &str, content: &[u8]) -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join(rel_path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).expect("create_dir_all");
        }
        std::fs::write(&file_path, content).expect("write");
        dir
    }

    /// Call `handle_copy` with a `CopySource::Host` and return the resulting
    /// (state, layer_summary) for further assertions.
    fn run_host_copy(
        state: &mut PreviewState,
        src: &str,
        dest: &str,
        context: &Path,
    ) -> LayerSummary {
        let source = CopySource::Host(PathBuf::from(src));
        let dest_path = PathBuf::from(dest);
        handle_copy(state, &source, &dest_path, context, 1, &StageRegistry::default())
            .expect("handle_copy ok")
    }

    // ── test: copy single file reads content ──────────────────────────────

    #[test]
    fn copy_single_file_reads_content() {
        let ctx = make_context_with_file("hello.txt", b"hello");
        let mut state = PreviewState::default();

        run_host_copy(&mut state, "hello.txt", "/app/hello.txt", ctx.path());

        let node = state
            .fs
            .get(Path::new("/app/hello.txt"))
            .expect("file must exist");
        match node {
            FsNode::File(f) => {
                assert_eq!(f.content, b"hello");
                match &f.provenance.created_by {
                    ProvenanceSource::CopyFromHost { host_path } => {
                        assert!(host_path.ends_with("hello.txt"));
                    }
                    other => panic!("unexpected provenance: {other:?}"),
                }
            }
            _ => panic!("expected File node"),
        }
    }

    // ── test: trailing slash dest appends source filename ─────────────────

    #[test]
    fn copy_single_file_to_dir_dest_appends_filename() {
        let ctx = make_context_with_file("hello.txt", b"hi");
        let mut state = PreviewState::default();

        // Dest has trailing slash — source filename must be appended.
        run_host_copy(&mut state, "hello.txt", "/app/", ctx.path());

        assert!(
            state.fs.contains(Path::new("/app/hello.txt")),
            "/app/hello.txt must exist when dest is /app/"
        );
    }

    // ── test: relative dest resolves against cwd ──────────────────────────

    #[test]
    fn copy_file_to_relative_dest_resolves_against_cwd() {
        let ctx = make_context_with_file("file.txt", b"data");
        let mut state = PreviewState::default();
        state.cwd = PathBuf::from("/work");

        run_host_copy(&mut state, "file.txt", "out/file.txt", ctx.path());

        assert!(
            state.fs.contains(Path::new("/work/out/file.txt")),
            "relative dest must resolve against cwd"
        );
    }

    // ── test: ancestor directories are created automatically ──────────────

    #[test]
    fn copy_creates_ancestor_directories() {
        let ctx = make_context_with_file("file.txt", b"x");
        let mut state = PreviewState::default();

        run_host_copy(&mut state, "file.txt", "/a/b/c/file.txt", ctx.path());

        assert!(state.fs.contains(Path::new("/a")), "/a must exist");
        assert!(state.fs.contains(Path::new("/a/b")), "/a/b must exist");
        assert!(state.fs.contains(Path::new("/a/b/c")), "/a/b/c must exist");
    }

    // ── test: missing source emits warning and inserts placeholder ────────

    #[test]
    fn copy_missing_source_emits_warning_and_inserts_placeholder() {
        let ctx = tempfile::tempdir().expect("tempdir");
        let mut state = PreviewState::default();

        run_host_copy(
            &mut state,
            "nonexistent.txt",
            "/app/missing.txt",
            ctx.path(),
        );

        // A MissingCopySource warning must be present.
        let has_warning = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::MissingCopySource { .. }));
        assert!(has_warning, "expected MissingCopySource warning");

        // An empty placeholder file must be inserted at the dest.
        let node = state
            .fs
            .get(Path::new("/app/missing.txt"))
            .expect("placeholder must exist");
        match node {
            FsNode::File(f) => assert!(f.content.is_empty(), "placeholder must be empty"),
            _ => panic!("expected File node for placeholder"),
        }
    }

    // ── test: directory copy walks all files recursively ─────────────────

    #[test]
    fn copy_directory_recursively_copies_all_files() {
        let ctx = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(ctx.path().join("src/b")).expect("create dirs");
        std::fs::write(ctx.path().join("src/a.txt"), b"a content").expect("write");
        std::fs::write(ctx.path().join("src/b/c.txt"), b"c content").expect("write");

        let mut state = PreviewState::default();
        run_host_copy(&mut state, "src", "/app", ctx.path());

        let a_node = state.fs.get(Path::new("/app/a.txt")).expect("/app/a.txt");
        match a_node {
            FsNode::File(f) => assert_eq!(f.content, b"a content"),
            _ => panic!("expected File"),
        }

        let c_node = state
            .fs
            .get(Path::new("/app/b/c.txt"))
            .expect("/app/b/c.txt");
        match c_node {
            FsNode::File(f) => assert_eq!(f.content, b"c content"),
            _ => panic!("expected File"),
        }
    }

    // ── test: directory copy with trailing slash dest ─────────────────────

    #[test]
    fn copy_directory_with_trailing_slash_dest() {
        // When source is a directory the trailing slash on dest has no effect
        // on directory copies — the files land at the same paths.
        let ctx = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(ctx.path().join("src/b")).expect("create dirs");
        std::fs::write(ctx.path().join("src/a.txt"), b"a").expect("write");
        std::fs::write(ctx.path().join("src/b/c.txt"), b"c").expect("write");

        let mut state = PreviewState::default();
        run_host_copy(&mut state, "src", "/app/", ctx.path());

        assert!(state.fs.contains(Path::new("/app/a.txt")));
        assert!(state.fs.contains(Path::new("/app/b/c.txt")));
    }

    // ── test: layer summary lists all files changed ───────────────────────

    #[test]
    fn copy_layer_summary_lists_all_files_changed() {
        let ctx = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(ctx.path().join("src")).expect("create dir");
        std::fs::write(ctx.path().join("src/x.txt"), b"x").expect("write");
        std::fs::write(ctx.path().join("src/y.txt"), b"y").expect("write");

        let mut state = PreviewState::default();
        let layer = run_host_copy(&mut state, "src", "/out", ctx.path());

        // files_changed must include both files (and possibly the directory entry).
        let has_x = layer.files_changed.iter().any(|p| p.ends_with("x.txt"));
        let has_y = layer.files_changed.iter().any(|p| p.ends_with("y.txt"));
        assert!(has_x, "x.txt must be in files_changed");
        assert!(has_y, "y.txt must be in files_changed");
    }

    // ── test: Stage copy with empty registry emits MissingCopyStage ──────

    #[test]
    fn copy_stage_source_with_empty_registry_emits_missing_stage_warning() {
        // When handle_copy is called with CopySource::Stage but the stage is
        // not in the registry, it must emit MissingCopyStage (not the old
        // UnsupportedInstruction placeholder) and return an empty LayerSummary.
        let ctx = tempfile::tempdir().expect("tempdir");
        let mut state = PreviewState::default();
        let source = CopySource::Stage {
            name: "builder".to_string(),
            src_path: std::path::PathBuf::from("/out/app"),
        };
        let dest = PathBuf::from("/app");
        let layer = handle_copy(
            &mut state,
            &source,
            &dest,
            ctx.path(),
            5,
            &StageRegistry::default(),
        )
        .expect("handle_copy ok");

        // No files should be copied — stage does not exist.
        assert!(
            layer.files_changed.is_empty(),
            "no files changed when stage is absent from registry"
        );

        // MissingCopyStage warning must be emitted with correct fields.
        let warning = state
            .warnings
            .iter()
            .find(|w| matches!(w, Warning::MissingCopyStage { stage, line: 5 } if stage == "builder"));
        assert!(
            warning.is_some(),
            "expected MissingCopyStage {{ stage: 'builder', line: 5 }} warning"
        );
    }

    // ── test: resolve_file_dest with no-filename source returns None ─────

    #[test]
    fn resolve_file_dest_returns_none_for_src_with_no_filename() {
        // `..` and `.` both have file_name() == None in Rust's Path API.
        // resolve_file_dest must return None rather than substituting the raw
        // OsStr, which was the original bug this fix addresses.
        let dest = PathBuf::from("/app");
        assert_eq!(
            resolve_file_dest(&dest, Path::new(".."), true),
            None,
            "trailing-slash dest with src `..` must yield None"
        );
        assert_eq!(
            resolve_file_dest(&dest, Path::new("."), true),
            None,
            "trailing-slash dest with src `.` must yield None"
        );
    }

    #[test]
    fn resolve_file_dest_no_trailing_slash_always_returns_dest() {
        // Without a trailing slash the dest is used as-is regardless of src_rel.
        let dest = PathBuf::from("/app/file.txt");
        assert_eq!(
            resolve_file_dest(&dest, Path::new(".."), false),
            Some(PathBuf::from("/app/file.txt")),
            "no trailing slash must always return dest"
        );
    }

    // ── Step 7 tests: stage-to-stage copy (RED until Step 8) ──────────────

    /// Helper: build a StageRegistry with one stage that contains `path` as a file.
    fn make_stage_with_file(
        alias: &str,
        path: &str,
        content: &[u8],
    ) -> crate::model::state::StageRegistry {
        use crate::model::{
            fs::{FileNode, FsNode},
            provenance::{Provenance, ProvenanceSource},
            state::StageRegistry,
        };

        let mut stage_state = PreviewState::default();
        // Ensure ancestor dirs exist in the stage state.
        let p = std::path::PathBuf::from(path);
        ensure_ancestors(
            &mut stage_state.fs,
            &p,
            ProvenanceSource::CopyFromHost {
                host_path: p.clone(),
            },
        );
        stage_state.fs.insert(
            p,
            FsNode::File(FileNode {
                content: content.to_vec(),
                provenance: Provenance::new(ProvenanceSource::CopyFromHost {
                    host_path: std::path::PathBuf::from(path),
                }),
                permissions: None,
            }),
        );

        let mut registry = StageRegistry::default();
        registry.insert(0, Some(alias), stage_state);
        registry
    }

    #[test]
    fn copy_from_stage_copies_file() {
        // A StageRegistry with one stage containing /out/app as a file.
        // handle_copy must place the file at /app/binary with CopyFromStage provenance.
        let registry = make_stage_with_file("builder", "/out/app", b"binary content");
        let ctx = tempfile::tempdir().expect("tempdir");
        let mut state = PreviewState::default();

        let source = CopySource::Stage {
            name: "builder".to_string(),
            src_path: std::path::PathBuf::from("/out/app"),
        };
        let dest = PathBuf::from("/app/binary");
        let layer = handle_copy(&mut state, &source, &dest, ctx.path(), 1, &registry)
            .expect("handle_copy ok");

        // File must exist at /app/binary.
        let node = state
            .fs
            .get(Path::new("/app/binary"))
            .expect("/app/binary must exist after stage copy");

        match node {
            FsNode::File(f) => {
                assert_eq!(f.content, b"binary content", "content must match source stage file");
                // Provenance must be CopyFromStage.
                match &f.provenance.created_by {
                    ProvenanceSource::CopyFromStage { stage } => {
                        assert_eq!(stage, "builder", "stage name must be 'builder'");
                    }
                    other => panic!("expected CopyFromStage provenance, got: {other:?}"),
                }
            }
            _ => panic!("expected File node at /app/binary"),
        }

        // files_changed must include the dest.
        assert!(
            layer.files_changed.contains(&PathBuf::from("/app/binary")),
            "files_changed must include /app/binary"
        );
    }

    #[test]
    fn copy_from_stage_missing_stage_emits_warning() {
        // Empty registry → stage "nonexistent" not found → MissingCopyStage warning.
        let registry = StageRegistry::default();
        let ctx = tempfile::tempdir().expect("tempdir");
        let mut state = PreviewState::default();

        let source = CopySource::Stage {
            name: "nonexistent".to_string(),
            src_path: std::path::PathBuf::from("/app"),
        };
        let layer = handle_copy(&mut state, &source, &PathBuf::from("/out"), ctx.path(), 3, &registry)
            .expect("handle_copy ok");

        // files_changed must be empty — nothing was copied.
        assert!(
            layer.files_changed.is_empty(),
            "no files should be changed when stage is missing"
        );

        // MissingCopyStage warning must be present.
        let has_warning = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::MissingCopyStage { stage, line: 3 } if stage == "nonexistent"));
        assert!(has_warning, "expected MissingCopyStage warning for 'nonexistent' at line 3");
    }

    #[test]
    fn copy_from_stage_missing_source_path_emits_warning() {
        // Stage exists but source path /out/missing is absent from its fs.
        let registry = make_stage_with_file("builder", "/out/app", b"data");
        let ctx = tempfile::tempdir().expect("tempdir");
        let mut state = PreviewState::default();

        let source = CopySource::Stage {
            name: "builder".to_string(),
            src_path: std::path::PathBuf::from("/out/missing"),
        };
        let _layer =
            handle_copy(&mut state, &source, &PathBuf::from("/app"), ctx.path(), 5, &registry)
                .expect("handle_copy ok");

        // MissingCopySource warning must be present.
        let has_warning = state
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::MissingCopySource { path } if path.ends_with("missing")));
        assert!(has_warning, "expected MissingCopySource warning for absent path in stage");
    }

    #[test]
    fn copy_from_stage_provenance_is_copy_from_stage() {
        // The copied file's provenance.created_by must be CopyFromStage { stage: "builder" }.
        let registry = make_stage_with_file("builder", "/out/app", b"hello");
        let ctx = tempfile::tempdir().expect("tempdir");
        let mut state = PreviewState::default();

        let source = CopySource::Stage {
            name: "builder".to_string(),
            src_path: std::path::PathBuf::from("/out/app"),
        };
        handle_copy(
            &mut state,
            &source,
            &PathBuf::from("/app/binary"),
            ctx.path(),
            1,
            &registry,
        )
        .expect("handle_copy ok");

        let node = state
            .fs
            .get(Path::new("/app/binary"))
            .expect("file must exist");
        match &node.provenance().created_by {
            ProvenanceSource::CopyFromStage { stage } => {
                assert_eq!(stage, "builder");
            }
            other => panic!("expected CopyFromStage, got {other:?}"),
        }
    }

    #[test]
    fn copy_from_stage_by_numeric_index() {
        // Stage stored under key "0" — lookup by name "0" must work.
        let registry = make_stage_with_file("builder", "/out/app", b"data");
        // The registry has both "0" and "builder" keys; test that "0" lookup works.
        let ctx = tempfile::tempdir().expect("tempdir");
        let mut state = PreviewState::default();

        let source = CopySource::Stage {
            name: "0".to_string(),
            src_path: std::path::PathBuf::from("/out/app"),
        };
        let layer = handle_copy(&mut state, &source, &PathBuf::from("/result"), ctx.path(), 1, &registry)
            .expect("handle_copy ok");

        assert!(
            !layer.files_changed.is_empty(),
            "numeric index '0' lookup must copy the file"
        );
        assert!(
            state.fs.contains(Path::new("/result")),
            "/result must exist after numeric-index stage copy"
        );
    }
}
