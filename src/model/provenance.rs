// Provenance tracking for every node in the virtual filesystem.
//
// Every file, directory, or symlink in VirtualFs carries a Provenance value
// that records where the node came from and what later instructions touched it.
// This powers the `:explain` REPL command.

use std::path::PathBuf;

/// Describes the Dockerfile instruction responsible for creating or modifying a node.
#[derive(Debug, Clone, PartialEq)]
pub enum ProvenanceSource {
    /// Node originates from the base image declared by FROM.
    FromImage { image: String },

    /// Node was created as a side effect of a WORKDIR instruction.
    Workdir,

    /// Node was copied from the host build context via COPY.
    CopyFromHost { host_path: PathBuf },

    /// Node was copied from a named build stage via `COPY --from=<stage>`.
    ///
    /// `stage` is the alias or numeric index string that identified the source stage.
    CopyFromStage { stage: String },

    /// Node was created or modified by a simulated RUN command.
    RunCommand { command: String },

    /// Node records an ENV key=value assignment.
    EnvSet { key: String, value: String },
}

/// Metadata about a volume mount that shadows a path in the virtual filesystem.
///
/// `MountInfo` records where a volume mounts inside the container (`container_path`),
/// whether it is read-only, and a pre-formatted human-readable `description` that is
/// used by both `:explain` and `:mounts`.  The description is computed from the
/// `VolumeSpec` variant at the time the mount shadow is applied by `engine::mount`.
#[derive(Debug, Clone, PartialEq)]
pub struct MountInfo {
    /// The absolute container path at which this volume is mounted.
    pub container_path: PathBuf,
    /// `true` when the mount was declared `read_only` in the Compose file.
    pub read_only: bool,
    /// Pre-formatted human-readable description of the mount source.
    ///
    /// Examples:
    /// - `"bind mount from ./data"`
    /// - `"named volume: npm-cache"`
    /// - `"anonymous volume"`
    pub description: String,
}

/// Provenance record attached to every `FsNode`.
///
/// Tracks how a node was originally created (`created_by`), any subsequent
/// instructions that modified it (`modified_by`), and whether a mount
/// currently shadows it (`shadowed_by_mount`).
#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    /// The instruction that first created this node.
    pub created_by: ProvenanceSource,

    /// Zero or more later instructions that modified this node after creation.
    pub modified_by: Vec<ProvenanceSource>,

    /// If a mount overlays this path, the mount descriptor is stored here.
    pub shadowed_by_mount: Option<MountInfo>,
}

impl Provenance {
    /// Create a new `Provenance` with the given creation source.
    ///
    /// `modified_by` starts empty and `shadowed_by_mount` starts as `None`.
    pub fn new(source: ProvenanceSource) -> Self {
        Self {
            created_by: source,
            modified_by: Vec::new(),
            shadowed_by_mount: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- ProvenanceSource construction ---

    #[test]
    fn provenance_source_from_image_stores_image_name() {
        let src = ProvenanceSource::FromImage {
            image: "ubuntu:22.04".to_string(),
        };
        assert_eq!(
            src,
            ProvenanceSource::FromImage {
                image: "ubuntu:22.04".to_string()
            }
        );
    }

    #[test]
    fn provenance_source_workdir_is_unit_variant() {
        let src = ProvenanceSource::Workdir;
        assert_eq!(src, ProvenanceSource::Workdir);
    }

    #[test]
    fn provenance_source_copy_from_host_stores_path() {
        let path = PathBuf::from("/host/build/context/app.py");
        let src = ProvenanceSource::CopyFromHost {
            host_path: path.clone(),
        };
        assert_eq!(src, ProvenanceSource::CopyFromHost { host_path: path });
    }

    #[test]
    fn provenance_source_copy_from_stage_stores_stage_name() {
        let src = ProvenanceSource::CopyFromStage {
            stage: "builder".to_string(),
        };
        assert_eq!(
            src,
            ProvenanceSource::CopyFromStage {
                stage: "builder".to_string()
            }
        );
    }

    #[test]
    fn provenance_source_run_command_stores_command_text() {
        let src = ProvenanceSource::RunCommand {
            command: "apt-get install -y curl".to_string(),
        };
        assert_eq!(
            src,
            ProvenanceSource::RunCommand {
                command: "apt-get install -y curl".to_string()
            }
        );
    }

    #[test]
    fn provenance_source_env_set_stores_key_and_value() {
        let src = ProvenanceSource::EnvSet {
            key: "PATH".to_string(),
            value: "/usr/local/bin:/usr/bin".to_string(),
        };
        assert_eq!(
            src,
            ProvenanceSource::EnvSet {
                key: "PATH".to_string(),
                value: "/usr/local/bin:/usr/bin".to_string()
            }
        );
    }

    // --- MountInfo construction ---

    #[test]
    fn mount_info_stores_container_path_and_description() {
        let mount = MountInfo {
            container_path: PathBuf::from("/data"),
            read_only: false,
            description: "bind mount from ./data".to_string(),
        };
        assert_eq!(mount.container_path, PathBuf::from("/data"));
        assert!(!mount.read_only);
        assert_eq!(mount.description, "bind mount from ./data");
    }

    #[test]
    fn mount_info_read_only_flag() {
        let mount = MountInfo {
            container_path: PathBuf::from("/cache"),
            read_only: true,
            description: "named volume: my-cache".to_string(),
        };
        assert!(mount.read_only);
    }

    #[test]
    fn mount_info_anonymous_volume_description() {
        let mount = MountInfo {
            container_path: PathBuf::from("/tmp/scratch"),
            read_only: false,
            description: "anonymous volume".to_string(),
        };
        assert_eq!(mount.description, "anonymous volume");
    }

    #[test]
    fn mount_info_clone_is_independent() {
        let original = MountInfo {
            container_path: PathBuf::from("/data"),
            read_only: false,
            description: "bind mount from ./data".to_string(),
        };
        let clone = original.clone();
        assert_eq!(original, clone);
    }

    // --- Provenance::new ---

    #[test]
    fn provenance_new_sets_created_by() {
        let src = ProvenanceSource::Workdir;
        let prov = Provenance::new(src.clone());
        assert_eq!(prov.created_by, src);
    }

    #[test]
    fn provenance_new_modified_by_is_empty() {
        let prov = Provenance::new(ProvenanceSource::Workdir);
        assert!(prov.modified_by.is_empty());
    }

    #[test]
    fn provenance_new_shadowed_by_mount_is_none() {
        let prov = Provenance::new(ProvenanceSource::Workdir);
        assert!(prov.shadowed_by_mount.is_none());
    }

    #[test]
    fn provenance_clone_produces_independent_copy() {
        let mut prov = Provenance::new(ProvenanceSource::Workdir);
        let clone = prov.clone();
        prov.modified_by.push(ProvenanceSource::RunCommand {
            command: "touch /tmp/x".to_string(),
        });
        // Cloned value must not see the mutation.
        assert!(clone.modified_by.is_empty());
    }

    #[test]
    fn provenance_equality_holds_for_identical_values() {
        let a = Provenance::new(ProvenanceSource::FromImage {
            image: "alpine:3.18".to_string(),
        });
        let b = Provenance::new(ProvenanceSource::FromImage {
            image: "alpine:3.18".to_string(),
        });
        assert_eq!(a, b);
    }

    #[test]
    fn provenance_inequality_when_sources_differ() {
        let a = Provenance::new(ProvenanceSource::Workdir);
        let b = Provenance::new(ProvenanceSource::FromImage {
            image: "alpine".to_string(),
        });
        assert_ne!(a, b);
    }
}
