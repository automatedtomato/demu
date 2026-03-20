// Mount shadow engine — applies Compose volume mounts to the preview state.
//
// `apply_mount_shadows` converts each `VolumeSpec` from a Compose service
// definition into a `MountInfo` record and:
//
// 1. Attaches the mount info to any existing FS node at the container path
//    (so `:explain` can report that a path is shadowed by a volume mount).
// 2. Appends the mount info to `state.mounts` (so `:mounts` can list all
//    active mount shadows).
//
// The host filesystem is never accessed — `host_path` values in `Bind`
// specs are used only as display strings in the mount description.

use std::path::Path;

use crate::model::compose::VolumeSpec;
use crate::model::provenance::MountInfo;
use crate::model::state::PreviewState;

/// Apply volume mount shadows from `volumes` to `state`.
///
/// For each `VolumeSpec`:
/// - A `MountInfo` is constructed with the container path, read-only flag,
///   and a human-readable description.
/// - If an FS node exists at the container path it is annotated with
///   `shadowed_by_mount`.
/// - The mount info is always recorded in `state.mounts` regardless of
///   whether a matching FS node exists.
pub fn apply_mount_shadows(state: &mut PreviewState, volumes: &[VolumeSpec]) {
    for vol in volumes {
        let info = build_mount_info(vol);
        let container_path = container_path_of(vol);

        // Shadow any FS node that already exists at this container path.
        if let Some(node) = state.fs.get_mut(container_path) {
            node.provenance_mut().shadowed_by_mount = Some(info.clone());
        }

        // Always record in the mounts list for `:mounts` display.
        state.mounts.push(info);
    }
}

/// Build a `MountInfo` from a `VolumeSpec`.
///
/// The `description` is computed once here so it can be stored in both
/// `Provenance::shadowed_by_mount` and `state.mounts` without needing
/// to reconstruct it at display time.
fn build_mount_info(spec: &VolumeSpec) -> MountInfo {
    match spec {
        VolumeSpec::Bind {
            host_path,
            container_path,
            read_only,
        } => MountInfo {
            container_path: container_path.clone(),
            read_only: *read_only,
            description: format!("bind mount from {}", host_path.display()),
        },
        VolumeSpec::Named {
            volume_name,
            container_path,
            read_only,
        } => MountInfo {
            container_path: container_path.clone(),
            read_only: *read_only,
            description: format!("named volume: {volume_name}"),
        },
        VolumeSpec::Anonymous { container_path } => MountInfo {
            container_path: container_path.clone(),
            read_only: false,
            description: "anonymous volume".to_string(),
        },
    }
}

/// Extract the container path from any `VolumeSpec` variant.
fn container_path_of(spec: &VolumeSpec) -> &Path {
    match spec {
        VolumeSpec::Bind { container_path, .. } => container_path,
        VolumeSpec::Named { container_path, .. } => container_path,
        VolumeSpec::Anonymous { container_path } => container_path,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::model::fs::{DirNode, FileNode, FsNode};
    use crate::model::provenance::{Provenance, ProvenanceSource};
    use crate::model::state::PreviewState;
    use std::path::PathBuf;

    fn workdir_provenance() -> Provenance {
        Provenance::new(ProvenanceSource::Workdir)
    }

    fn state_with_dir(path: &str) -> PreviewState {
        let mut state = PreviewState::default();
        let node = FsNode::Directory(DirNode {
            provenance: workdir_provenance(),
            permissions: None,
        });
        state.fs.insert(PathBuf::from(path), node);
        state
    }

    fn state_with_file(path: &str) -> PreviewState {
        let mut state = PreviewState::default();
        let node = FsNode::File(FileNode {
            content: vec![],
            provenance: workdir_provenance(),
            permissions: None,
        });
        state.fs.insert(PathBuf::from(path), node);
        state
    }

    // --- apply_mount_shadows: records mounts in state.mounts ---

    #[test]
    fn bind_spec_is_recorded_in_state_mounts() {
        let mut state = PreviewState::default();
        let volumes = vec![VolumeSpec::Bind {
            host_path: PathBuf::from("./data"),
            container_path: PathBuf::from("/data"),
            read_only: false,
        }];
        apply_mount_shadows(&mut state, &volumes);
        assert_eq!(state.mounts.len(), 1);
        assert_eq!(state.mounts[0].container_path, PathBuf::from("/data"));
        // PathBuf strips leading "./" — "bind mount from data" is expected.
        assert!(
            state.mounts[0].description.starts_with("bind mount from"),
            "got: {}",
            state.mounts[0].description
        );
        assert!(!state.mounts[0].read_only);
    }

    #[test]
    fn named_spec_is_recorded_in_state_mounts() {
        let mut state = PreviewState::default();
        let volumes = vec![VolumeSpec::Named {
            volume_name: "npm-cache".to_string(),
            container_path: PathBuf::from("/root/.npm"),
            read_only: false,
        }];
        apply_mount_shadows(&mut state, &volumes);
        assert_eq!(state.mounts.len(), 1);
        assert_eq!(state.mounts[0].description, "named volume: npm-cache");
        assert_eq!(state.mounts[0].container_path, PathBuf::from("/root/.npm"));
    }

    #[test]
    fn anonymous_spec_is_recorded_in_state_mounts() {
        let mut state = PreviewState::default();
        let volumes = vec![VolumeSpec::Anonymous {
            container_path: PathBuf::from("/tmp/scratch"),
        }];
        apply_mount_shadows(&mut state, &volumes);
        assert_eq!(state.mounts.len(), 1);
        assert_eq!(state.mounts[0].description, "anonymous volume");
        assert!(!state.mounts[0].read_only);
    }

    #[test]
    fn read_only_bind_sets_read_only_true_in_mount_info() {
        let mut state = PreviewState::default();
        let volumes = vec![VolumeSpec::Bind {
            host_path: PathBuf::from("./config"),
            container_path: PathBuf::from("/config"),
            read_only: true,
        }];
        apply_mount_shadows(&mut state, &volumes);
        assert!(state.mounts[0].read_only);
    }

    #[test]
    fn read_only_named_sets_read_only_true_in_mount_info() {
        let mut state = PreviewState::default();
        let volumes = vec![VolumeSpec::Named {
            volume_name: "shared-data".to_string(),
            container_path: PathBuf::from("/shared"),
            read_only: true,
        }];
        apply_mount_shadows(&mut state, &volumes);
        assert!(state.mounts[0].read_only);
    }

    // --- apply_mount_shadows: shadows existing FS nodes ---

    #[test]
    fn existing_dir_node_gets_shadowed_by_bind_mount() {
        let mut state = state_with_dir("/data");
        let volumes = vec![VolumeSpec::Bind {
            host_path: PathBuf::from("./data"),
            container_path: PathBuf::from("/data"),
            read_only: false,
        }];
        apply_mount_shadows(&mut state, &volumes);

        let node = state
            .fs
            .get(&PathBuf::from("/data"))
            .expect("/data must exist");
        let shadow = node
            .provenance()
            .shadowed_by_mount
            .as_ref()
            .expect("must be shadowed");
        assert_eq!(shadow.container_path, PathBuf::from("/data"));
        assert!(shadow.description.contains("bind mount from"));
    }

    #[test]
    fn existing_file_node_gets_shadowed_by_named_mount() {
        let mut state = state_with_file("/app/config.json");
        let volumes = vec![VolumeSpec::Named {
            volume_name: "config-vol".to_string(),
            container_path: PathBuf::from("/app/config.json"),
            read_only: true,
        }];
        apply_mount_shadows(&mut state, &volumes);

        let node = state
            .fs
            .get(&PathBuf::from("/app/config.json"))
            .expect("must exist");
        let shadow = node
            .provenance()
            .shadowed_by_mount
            .as_ref()
            .expect("must be shadowed");
        assert_eq!(shadow.description, "named volume: config-vol");
        assert!(shadow.read_only);
    }

    // --- apply_mount_shadows: nonexistent path still records mount ---

    #[test]
    fn nonexistent_container_path_still_records_in_state_mounts() {
        // The path doesn't exist in the FS — no node to shadow — but
        // the mount must still appear in state.mounts for `:mounts`.
        let mut state = PreviewState::default();
        let volumes = vec![VolumeSpec::Bind {
            host_path: PathBuf::from("./uploads"),
            container_path: PathBuf::from("/uploads"),
            read_only: false,
        }];
        apply_mount_shadows(&mut state, &volumes);
        assert_eq!(
            state.mounts.len(),
            1,
            "mount must be recorded even without an FS node"
        );
        // No node at /uploads — FS remains empty.
        assert!(state.fs.get(&PathBuf::from("/uploads")).is_none());
    }

    // --- apply_mount_shadows: multiple volumes ---

    #[test]
    fn multiple_volumes_all_recorded() {
        let mut state = PreviewState::default();
        let volumes = vec![
            VolumeSpec::Bind {
                host_path: PathBuf::from("./data"),
                container_path: PathBuf::from("/data"),
                read_only: false,
            },
            VolumeSpec::Named {
                volume_name: "cache".to_string(),
                container_path: PathBuf::from("/cache"),
                read_only: false,
            },
            VolumeSpec::Anonymous {
                container_path: PathBuf::from("/tmp"),
            },
        ];
        apply_mount_shadows(&mut state, &volumes);
        assert_eq!(state.mounts.len(), 3);
    }

    #[test]
    fn empty_volumes_list_leaves_state_unchanged() {
        let mut state = PreviewState::default();
        apply_mount_shadows(&mut state, &[]);
        assert!(state.mounts.is_empty());
    }
}
