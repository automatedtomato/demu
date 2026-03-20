// Answers provenance questions about files in the virtual filesystem.
// Powers the `:explain` REPL command.
//
// The entry point is `explain_path`, which accepts a `PreviewState` and a
// `Path` and returns a multi-line human-readable provenance report.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::model::{
    provenance::{MountInfo, ProvenanceSource},
    state::PreviewState,
};
use crate::output::sanitize::sanitize_for_terminal;

/// Error type for the explain module.
#[derive(Debug, PartialEq)]
pub enum ExplainError {
    /// The queried path does not exist in the virtual filesystem.
    PathNotFound { path: PathBuf },
}

impl fmt::Display for ExplainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExplainError::PathNotFound { path } => {
                write!(f, "path not found: {}", path.display())
            }
        }
    }
}

/// Column width of the section labels (e.g. `"Modified by:  "` is 14 chars).
///
/// Subsequent modification entries are indented by this many spaces so they
/// align under the first entry on the line that starts with the label.
const LABEL_WIDTH: usize = 14;

/// Format a human-readable provenance report for the node at `path`.
///
/// Resolves the path against the virtual filesystem and returns a
/// multi-line string describing how the node was created, modified,
/// and whether it is currently shadowed by a mount.
///
/// Returns `ExplainError::PathNotFound` when the path does not exist
/// in the virtual filesystem.
pub fn explain_path(state: &PreviewState, path: &Path) -> Result<String, ExplainError> {
    // Look up the node in the virtual filesystem using the exact path provided.
    let node = state
        .fs
        .get(path)
        .ok_or_else(|| ExplainError::PathNotFound {
            path: path.to_path_buf(),
        })?;

    let prov = node.provenance();

    // Format the "Created by:" line using the creation source.
    let created_line = format!("Created by:   {}", format_source(&prov.created_by));

    // Format the "Modified by:" section.
    // When empty, show "(none)". When multiple entries exist, subsequent lines
    // are indented by 14 spaces to align under the first modification entry.
    let modified_line = if prov.modified_by.is_empty() {
        "Modified by:  (none)".to_string()
    } else {
        let mut parts: Vec<String> = prov.modified_by.iter().map(format_source).collect();

        // First entry gets the label; subsequent entries are indented by
        // LABEL_WIDTH spaces to align under the first value.
        let first = format!("Modified by:  {}", parts.remove(0));
        let padding = " ".repeat(LABEL_WIDTH);
        let rest: Vec<String> = parts.into_iter().map(|s| format!("{padding}{s}")).collect();

        if rest.is_empty() {
            first
        } else {
            format!("{}\n{}", first, rest.join("\n"))
        }
    };

    // Format the "Shadowed by:" line from the optional mount descriptor.
    let shadowed_line = match &prov.shadowed_by_mount {
        None => "Shadowed by:  (none)".to_string(),
        Some(mount) => format!("Shadowed by:  {}", format_mount(mount)),
    };

    Ok(format!("{created_line}\n{modified_line}\n{shadowed_line}"))
}

/// Format a single `ProvenanceSource` as a human-readable string.
///
/// Each variant maps to a compact description that fits on one terminal line.
/// User-supplied fields (image names, paths, command text, env keys/values,
/// stage names) are sanitized with `sanitize_for_terminal` to prevent ANSI
/// escape injection when the result is written to a terminal.
fn format_source(source: &ProvenanceSource) -> String {
    match source {
        ProvenanceSource::FromImage { image } => {
            format!("base image: {}", sanitize_for_terminal(image))
        }
        ProvenanceSource::Workdir => "WORKDIR instruction".to_string(),
        ProvenanceSource::CopyFromHost { host_path } => {
            let safe = sanitize_for_terminal(&host_path.display().to_string());
            format!("COPY from host: {safe}")
        }
        ProvenanceSource::CopyFromStage { stage } => {
            format!("COPY from stage '{}'", sanitize_for_terminal(stage))
        }
        ProvenanceSource::RunCommand { command } => {
            format!("RUN command: {}", sanitize_for_terminal(command))
        }
        ProvenanceSource::EnvSet { key, value } => {
            format!(
                "ENV {}={}",
                sanitize_for_terminal(key),
                sanitize_for_terminal(value)
            )
        }
    }
}

/// Format a `MountInfo` as a human-readable string.
///
/// Returns the pre-formatted `description` field, sanitizing it against
/// ANSI escape injection before emitting it to the terminal.
fn format_mount(mount: &MountInfo) -> String {
    sanitize_for_terminal(&mount.description)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::{
        fs::{FileNode, FsNode},
        provenance::{MountInfo, Provenance, ProvenanceSource},
        state::PreviewState,
    };
    use std::path::PathBuf;

    // Helper: insert a file node with the given provenance into state at path.
    fn state_with_node(path: &str, prov: Provenance) -> PreviewState {
        let mut state = PreviewState::default();
        let node = FsNode::File(FileNode {
            content: vec![],
            provenance: prov,
            permissions: None,
        });
        state.fs.insert(PathBuf::from(path), node);
        state
    }

    // --- explain_copy_from_host_shows_created_by ---

    #[test]
    fn explain_copy_from_host_shows_created_by() {
        let prov = Provenance::new(ProvenanceSource::CopyFromHost {
            host_path: PathBuf::from("src/main.rs"),
        });
        let state = state_with_node("/app/main.rs", prov);
        let output = explain_path(&state, Path::new("/app/main.rs")).expect("should succeed");
        assert!(
            output.contains("COPY from host: src/main.rs"),
            "got: {output}"
        );
    }

    // --- explain_workdir_shows_workdir_instruction ---

    #[test]
    fn explain_workdir_shows_workdir_instruction() {
        let prov = Provenance::new(ProvenanceSource::Workdir);
        let state = state_with_node("/app", prov);
        let output = explain_path(&state, Path::new("/app")).expect("should succeed");
        assert!(output.contains("WORKDIR instruction"), "got: {output}");
    }

    // --- explain_run_command_shows_run_command ---

    #[test]
    fn explain_run_command_shows_run_command() {
        let prov = Provenance::new(ProvenanceSource::RunCommand {
            command: "touch /app/flag".to_string(),
        });
        let state = state_with_node("/app/flag", prov);
        let output = explain_path(&state, Path::new("/app/flag")).expect("should succeed");
        assert!(
            output.contains("RUN command: touch /app/flag"),
            "got: {output}"
        );
    }

    // --- explain_from_image_shows_base_image ---

    #[test]
    fn explain_from_image_shows_base_image() {
        let prov = Provenance::new(ProvenanceSource::FromImage {
            image: "ubuntu:22.04".to_string(),
        });
        let state = state_with_node("/etc/os-release", prov);
        let output = explain_path(&state, Path::new("/etc/os-release")).expect("should succeed");
        assert!(output.contains("base image: ubuntu:22.04"), "got: {output}");
    }

    // --- explain_env_set_shows_env_assignment ---

    #[test]
    fn explain_env_set_shows_env_assignment() {
        let prov = Provenance::new(ProvenanceSource::EnvSet {
            key: "PATH".to_string(),
            value: "/usr/local/bin:/usr/bin".to_string(),
        });
        let state = state_with_node("/env/PATH", prov);
        let output = explain_path(&state, Path::new("/env/PATH")).expect("should succeed");
        assert!(
            output.contains("ENV PATH=/usr/local/bin:/usr/bin"),
            "got: {output}"
        );
    }

    // --- explain_copy_from_stage_shows_stage_name ---

    #[test]
    fn explain_copy_from_stage_shows_stage_name() {
        let prov = Provenance::new(ProvenanceSource::CopyFromStage {
            stage: "builder".to_string(),
        });
        let state = state_with_node("/app/binary", prov);
        let output = explain_path(&state, Path::new("/app/binary")).expect("should succeed");
        assert!(
            output.contains("COPY from stage 'builder'"),
            "got: {output}"
        );
    }

    // --- explain_with_modifications_shows_all_entries ---

    #[test]
    fn explain_with_modifications_shows_all_entries() {
        let mut prov = Provenance::new(ProvenanceSource::CopyFromHost {
            host_path: PathBuf::from("app.py"),
        });
        prov.modified_by.push(ProvenanceSource::RunCommand {
            command: "chmod 755 /app/app.py".to_string(),
        });
        let state = state_with_node("/app/app.py", prov);
        let output = explain_path(&state, Path::new("/app/app.py")).expect("should succeed");
        assert!(output.contains("COPY from host: app.py"), "got: {output}");
        assert!(
            output.contains("RUN command: chmod 755 /app/app.py"),
            "got: {output}"
        );
    }

    // --- explain_with_mount_shows_mount_info ---

    #[test]
    fn explain_with_mount_shows_mount_info() {
        let mut prov = Provenance::new(ProvenanceSource::CopyFromHost {
            host_path: PathBuf::from("src/main.rs"),
        });
        prov.shadowed_by_mount = Some(MountInfo {
            container_path: PathBuf::from("/app/main.rs"),
            read_only: false,
            description: "bind mount from /host/path".to_string(),
        });
        let state = state_with_node("/app/main.rs", prov);
        let output = explain_path(&state, Path::new("/app/main.rs")).expect("should succeed");
        assert!(
            output.contains("bind mount from /host/path"),
            "got: {output}"
        );
    }

    // --- explain_path_not_found_returns_error ---

    #[test]
    fn explain_path_not_found_returns_error() {
        let state = PreviewState::default();
        let result = explain_path(&state, Path::new("/nonexistent/file.txt"));
        assert_eq!(
            result,
            Err(ExplainError::PathNotFound {
                path: PathBuf::from("/nonexistent/file.txt")
            })
        );
    }

    // --- explain_modified_by_none_shows_none ---

    #[test]
    fn explain_modified_by_none_shows_none() {
        let prov = Provenance::new(ProvenanceSource::Workdir);
        // modified_by is empty by default from Provenance::new
        let state = state_with_node("/app", prov);
        let output = explain_path(&state, Path::new("/app")).expect("should succeed");
        assert!(output.contains("Modified by:  (none)"), "got: {output}");
    }

    // --- explain_shadowed_by_none_shows_none ---

    #[test]
    fn explain_shadowed_by_none_shows_none() {
        let prov = Provenance::new(ProvenanceSource::Workdir);
        // shadowed_by_mount is None by default
        let state = state_with_node("/app", prov);
        let output = explain_path(&state, Path::new("/app")).expect("should succeed");
        assert!(output.contains("Shadowed by:  (none)"), "got: {output}");
    }

    // --- explain_multiple_modifications_aligned ---
    //
    // When there are 2+ modifications, subsequent lines must be indented by
    // 14 spaces so they align under the first modification entry.

    #[test]
    fn explain_multiple_modifications_aligned() {
        let mut prov = Provenance::new(ProvenanceSource::CopyFromHost {
            host_path: PathBuf::from("app.py"),
        });
        prov.modified_by.push(ProvenanceSource::RunCommand {
            command: "mv /app/app.py /app/main.py".to_string(),
        });
        prov.modified_by.push(ProvenanceSource::RunCommand {
            command: "chmod 755 /app/main.py".to_string(),
        });
        let state = state_with_node("/app/main.py", prov);
        let output = explain_path(&state, Path::new("/app/main.py")).expect("should succeed");

        // Both modifications must appear.
        assert!(
            output.contains("RUN command: mv /app/app.py /app/main.py"),
            "first modification missing; got: {output}"
        );
        assert!(
            output.contains("RUN command: chmod 755 /app/main.py"),
            "second modification missing; got: {output}"
        );

        // The second modification must be indented by 14 spaces.
        // The label "Modified by:  " is 14 chars; subsequent lines align under the value.
        assert!(
            output.contains("              RUN command: chmod 755 /app/main.py"),
            "second modification must be indented 14 spaces; got:\n{output}"
        );
    }

    // --- mount description format tests ---

    #[test]
    fn format_mount_returns_description() {
        let mount = MountInfo {
            container_path: PathBuf::from("/data"),
            read_only: false,
            description: "named volume: myvolume".to_string(),
        };
        assert_eq!(format_mount(&mount), "named volume: myvolume");
    }

    #[test]
    fn format_mount_anonymous_volume() {
        let mount = MountInfo {
            container_path: PathBuf::from("/tmp"),
            read_only: false,
            description: "anonymous volume".to_string(),
        };
        assert_eq!(format_mount(&mount), "anonymous volume");
    }

    #[test]
    fn format_mount_bind_description() {
        let mount = MountInfo {
            container_path: PathBuf::from("/app"),
            read_only: true,
            description: "bind mount from ./src".to_string(),
        };
        assert_eq!(format_mount(&mount), "bind mount from ./src");
    }

    // --- sanitization of user-controlled fields ---

    #[test]
    fn format_source_strips_ansi_escape_in_run_command() {
        // A crafted RUN command containing an ANSI escape sequence must not
        // pass raw ESC bytes through to the terminal output.
        let source = ProvenanceSource::RunCommand {
            command: "touch \x1b[2J /app/flag".to_string(),
        };
        let result = format_source(&source);
        assert!(
            !result.contains('\x1b'),
            "ESC byte must be stripped; got: {result:?}"
        );
        assert!(
            result.contains("RUN command:"),
            "label must survive; got: {result:?}"
        );
    }

    #[test]
    fn format_source_strips_ansi_escape_in_image_name() {
        let source = ProvenanceSource::FromImage {
            image: "ubuntu\x1b[1m:22.04".to_string(),
        };
        let result = format_source(&source);
        assert!(
            !result.contains('\x1b'),
            "ESC byte must be stripped; got: {result:?}"
        );
    }

    #[test]
    fn format_mount_strips_ansi_escape_in_description() {
        let mount = MountInfo {
            container_path: PathBuf::from("/app"),
            read_only: false,
            description: "bind mount from /host/\x1b[2Jpath".to_string(),
        };
        let result = format_mount(&mount);
        assert!(
            !result.contains('\x1b'),
            "ESC byte must be stripped; got: {result:?}"
        );
    }

    // --- ExplainError::Display ---

    #[test]
    fn explain_error_display_contains_path() {
        let err = ExplainError::PathNotFound {
            path: PathBuf::from("/missing/file"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/missing/file"), "got: {msg}");
    }
}
