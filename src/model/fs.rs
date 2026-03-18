// Virtual filesystem types used by the simulation engine.
//
// The virtual filesystem is a flat HashMap from absolute PathBuf to FsNode.
// There is no in-memory tree structure; directory relationships are inferred
// from path prefixes at query time. This keeps the data model simple and
// makes it easy to serialise and inspect in tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::provenance::Provenance;

/// A regular file node in the virtual filesystem.
#[derive(Debug, Clone, PartialEq)]
pub struct FileNode {
    /// Raw byte content of the file. Empty for placeholder files.
    pub content: Vec<u8>,

    /// Where this file came from (must always be present).
    pub provenance: Provenance,

    /// Optional Unix permission bits (e.g. 0o644). None means "not modeled".
    pub permissions: Option<u32>,
}

/// A directory node in the virtual filesystem.
#[derive(Debug, Clone, PartialEq)]
pub struct DirNode {
    /// Where this directory came from.
    pub provenance: Provenance,

    /// Optional Unix permission bits. None means "not modeled".
    pub permissions: Option<u32>,
}

/// A symbolic link node in the virtual filesystem.
#[derive(Debug, Clone, PartialEq)]
pub struct SymlinkNode {
    /// The path this symlink points to (may be relative or absolute).
    pub target: PathBuf,

    /// Where this symlink came from.
    pub provenance: Provenance,
}

/// Any node that can exist at a path in the virtual filesystem.
#[derive(Debug, Clone, PartialEq)]
pub enum FsNode {
    /// A regular file.
    File(FileNode),
    /// A directory entry.
    Directory(DirNode),
    /// A symbolic link.
    Symlink(SymlinkNode),
}

impl FsNode {
    /// Return a reference to the provenance record embedded in this node.
    ///
    /// Every FsNode variant carries a provenance value; this method provides
    /// uniform access without requiring callers to pattern-match on the variant.
    pub fn provenance(&self) -> &Provenance {
        match self {
            FsNode::File(f) => &f.provenance,
            FsNode::Directory(d) => &d.provenance,
            FsNode::Symlink(s) => &s.provenance,
        }
    }
}

/// An in-memory virtual filesystem backed by a flat path-to-node map.
///
/// All paths stored in `VirtualFs` must have a leading `/`. The `insert`
/// method does not enforce this automatically — callers are responsible for
/// normalising paths before insertion. Query methods accept any `Path`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VirtualFs {
    nodes: HashMap<PathBuf, FsNode>,
}

impl VirtualFs {
    /// Create an empty virtual filesystem.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    /// Insert a node at the given path, overwriting any existing node.
    ///
    /// # Panics (debug builds only)
    ///
    /// Panics in debug mode if `path` is not absolute. All paths stored in
    /// `VirtualFs` must be absolute so that `list_dir`, `get`, and `contains`
    /// produce correct results. In release builds the assertion is removed.
    pub fn insert(&mut self, path: PathBuf, node: FsNode) {
        debug_assert!(
            path.is_absolute(),
            "VirtualFs::insert requires an absolute path, got: {}",
            path.display()
        );
        self.nodes.insert(path, node);
    }

    /// Return a reference to the node at `path`, or `None` if it does not exist.
    pub fn get(&self, path: &Path) -> Option<&FsNode> {
        self.nodes.get(path)
    }

    /// Return `true` if a node exists at `path`.
    pub fn contains(&self, path: &Path) -> bool {
        self.nodes.contains_key(path)
    }

    /// Remove and return the node at `path`, or `None` if it was not present.
    pub fn remove(&mut self, path: &Path) -> Option<FsNode> {
        self.nodes.remove(path)
    }

    /// Return the direct children of `dir`.
    ///
    /// A path `p` is a direct child of `dir` when:
    /// - `p` starts with `dir` as a prefix, AND
    /// - `p` has exactly one more path component beyond `dir`.
    ///
    /// The root directory `/` is treated specially: a path like `/foo` is a
    /// direct child, but `/foo/bar` is not.
    ///
    /// Returns an empty vec when `dir` does not exist or has no children.
    pub fn list_dir(&self, dir: &Path) -> Vec<(PathBuf, &FsNode)> {
        self.nodes
            .iter()
            .filter(|(path, _)| is_direct_child(dir, path))
            .map(|(path, node)| (path.clone(), node))
            .collect()
    }

    /// Iterate over all (path, node) pairs in the filesystem.
    pub fn iter(&self) -> impl Iterator<Item = (&PathBuf, &FsNode)> {
        self.nodes.iter()
    }
}

/// Return `true` when `candidate` is a direct child of `parent`.
///
/// A direct child has exactly one more component than its parent. Both paths
/// are assumed to be absolute.
fn is_direct_child(parent: &Path, candidate: &Path) -> bool {
    // Strip the parent prefix; if it fails this path is not under parent at all.
    let Ok(suffix) = candidate.strip_prefix(parent) else {
        return false;
    };

    // A direct child has exactly one component in the suffix (e.g. "foo" but not "foo/bar").
    let mut components = suffix.components();
    components.next().is_some() && components.next().is_none()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::model::provenance::{Provenance, ProvenanceSource};
    use std::path::PathBuf;

    // Helper: create a minimal FileNode with Workdir provenance.
    fn file_node(content: &[u8]) -> FsNode {
        FsNode::File(FileNode {
            content: content.to_vec(),
            provenance: Provenance::new(ProvenanceSource::Workdir),
            permissions: None,
        })
    }

    // Helper: create a minimal DirNode.
    fn dir_node() -> FsNode {
        FsNode::Directory(DirNode {
            provenance: Provenance::new(ProvenanceSource::Workdir),
            permissions: None,
        })
    }

    // Helper: create a minimal SymlinkNode.
    fn symlink_node(target: &str) -> FsNode {
        FsNode::Symlink(SymlinkNode {
            target: PathBuf::from(target),
            provenance: Provenance::new(ProvenanceSource::Workdir),
        })
    }

    // --- VirtualFs::new ---

    #[test]
    fn new_virtual_fs_is_empty() {
        let fs = VirtualFs::new();
        assert!(!fs.contains(Path::new("/any")));
    }

    // --- insert / get ---

    #[test]
    fn insert_then_get_returns_same_node() {
        let mut fs = VirtualFs::new();
        let node = file_node(b"hello");
        fs.insert(PathBuf::from("/app/main.rs"), node.clone());
        let retrieved = fs.get(Path::new("/app/main.rs"));
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &node);
    }

    #[test]
    fn get_returns_none_for_absent_path() {
        let fs = VirtualFs::new();
        assert!(fs.get(Path::new("/not/there")).is_none());
    }

    // --- contains ---

    #[test]
    fn contains_returns_false_before_insert() {
        let fs = VirtualFs::new();
        assert!(!fs.contains(Path::new("/app")));
    }

    #[test]
    fn contains_returns_true_after_insert() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        assert!(fs.contains(Path::new("/app")));
    }

    // --- insert overwrites ---

    #[test]
    fn inserting_same_path_twice_overwrites_first_node() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app/file.txt"), file_node(b"first"));
        fs.insert(PathBuf::from("/app/file.txt"), file_node(b"second"));

        let node = fs.get(Path::new("/app/file.txt")).expect("node must exist");
        match node {
            FsNode::File(f) => assert_eq!(f.content, b"second"),
            _ => panic!("expected File node"),
        }
    }

    // --- remove ---

    #[test]
    fn remove_returns_node_and_path_no_longer_contained() {
        let mut fs = VirtualFs::new();
        let node = file_node(b"data");
        fs.insert(PathBuf::from("/app/x"), node.clone());
        let removed = fs.remove(Path::new("/app/x"));
        assert_eq!(removed, Some(node));
        assert!(!fs.contains(Path::new("/app/x")));
    }

    #[test]
    fn remove_absent_path_returns_none() {
        let mut fs = VirtualFs::new();
        assert!(fs.remove(Path::new("/nothing")).is_none());
    }

    // --- list_dir ---

    #[test]
    fn list_dir_root_returns_only_direct_children() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        fs.insert(PathBuf::from("/etc"), dir_node());
        // This is a grandchild of / — should NOT appear in list_dir("/")
        fs.insert(PathBuf::from("/app/main.rs"), file_node(b""));

        let children = fs.list_dir(Path::new("/"));
        let paths: Vec<PathBuf> = children.iter().map(|(p, _)| p.clone()).collect();

        assert!(paths.contains(&PathBuf::from("/app")), "expected /app");
        assert!(paths.contains(&PathBuf::from("/etc")), "expected /etc");
        assert!(
            !paths.contains(&PathBuf::from("/app/main.rs")),
            "/app/main.rs must not appear as a direct child of /"
        );
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn list_dir_app_returns_children_of_app_only() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        fs.insert(PathBuf::from("/app/main.rs"), file_node(b""));
        fs.insert(PathBuf::from("/app/lib.rs"), file_node(b""));
        // Deep grandchild — must not appear
        fs.insert(PathBuf::from("/app/src/util.rs"), file_node(b""));
        // Sibling of /app — must not appear
        fs.insert(PathBuf::from("/etc/hosts"), file_node(b""));

        let children = fs.list_dir(Path::new("/app"));
        let paths: Vec<PathBuf> = children.iter().map(|(p, _)| p.clone()).collect();

        assert!(paths.contains(&PathBuf::from("/app/main.rs")));
        assert!(paths.contains(&PathBuf::from("/app/lib.rs")));
        assert!(!paths.contains(&PathBuf::from("/app/src/util.rs")));
        assert!(!paths.contains(&PathBuf::from("/etc/hosts")));
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn list_dir_returns_empty_for_unknown_dir() {
        let fs = VirtualFs::new();
        let children = fs.list_dir(Path::new("/nonexistent"));
        assert!(children.is_empty());
    }

    #[test]
    fn list_dir_does_not_match_string_prefix_sibling() {
        // /app is a string prefix of /appdata, but NOT a path-component ancestor.
        // strip_prefix uses component-level comparison, so /appdata/file must NOT
        // appear in list_dir("/app"). A naive str::starts_with implementation
        // would produce a false positive here.
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/appdata/file.txt"), file_node(b""));
        let children = fs.list_dir(Path::new("/app"));
        assert!(
            children.is_empty(),
            "/appdata/file.txt must not appear as a child of /app"
        );
    }

    #[test]
    fn list_dir_does_not_include_the_queried_directory_itself() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        let children = fs.list_dir(Path::new("/app"));
        let paths: Vec<PathBuf> = children.iter().map(|(p, _)| p.clone()).collect();
        assert!(!paths.contains(&PathBuf::from("/app")));
    }

    #[test]
    fn list_dir_returns_empty_when_dir_has_no_children() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/empty"), dir_node());
        let children = fs.list_dir(Path::new("/empty"));
        assert!(children.is_empty());
    }

    // --- iter ---

    #[test]
    fn iter_visits_all_inserted_nodes() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/a"), file_node(b""));
        fs.insert(PathBuf::from("/b"), file_node(b""));
        fs.insert(PathBuf::from("/c"), dir_node());

        let collected: Vec<&PathBuf> = fs.iter().map(|(p, _)| p).collect();
        assert_eq!(collected.len(), 3);
    }

    #[test]
    fn iter_on_empty_fs_produces_no_items() {
        let fs = VirtualFs::new();
        assert_eq!(fs.iter().count(), 0);
    }

    // --- FsNode::provenance ---

    #[test]
    fn file_node_provenance_returns_embedded_provenance() {
        let src = ProvenanceSource::CopyFromHost {
            host_path: PathBuf::from("/build/app.py"),
        };
        let prov = Provenance::new(src.clone());
        let node = FsNode::File(FileNode {
            content: vec![],
            provenance: prov,
            permissions: None,
        });
        assert_eq!(node.provenance().created_by, src);
    }

    #[test]
    fn dir_node_provenance_returns_embedded_provenance() {
        let src = ProvenanceSource::Workdir;
        let prov = Provenance::new(src.clone());
        let node = FsNode::Directory(DirNode {
            provenance: prov,
            permissions: None,
        });
        assert_eq!(node.provenance().created_by, src);
    }

    #[test]
    fn symlink_node_provenance_returns_embedded_provenance() {
        let src = ProvenanceSource::RunCommand {
            command: "ln -s /usr/bin/python3 /usr/local/bin/python".to_string(),
        };
        let prov = Provenance::new(src.clone());
        let node = FsNode::Symlink(SymlinkNode {
            target: PathBuf::from("/usr/bin/python3"),
            provenance: prov,
        });
        assert_eq!(node.provenance().created_by, src);
    }

    // --- Clone and PartialEq ---

    #[test]
    fn virtual_fs_clone_is_independent_copy() {
        let mut fs = VirtualFs::new();
        fs.insert(PathBuf::from("/app"), dir_node());
        let mut clone = fs.clone();
        clone.insert(PathBuf::from("/tmp"), dir_node());
        // Original should not contain /tmp
        assert!(!fs.contains(Path::new("/tmp")));
    }

    #[test]
    fn two_empty_virtual_fs_are_equal() {
        assert_eq!(VirtualFs::new(), VirtualFs::new());
    }

    #[test]
    fn symlink_node_stores_target() {
        let node = symlink_node("/usr/bin/python3");
        match node {
            FsNode::Symlink(s) => assert_eq!(s.target, PathBuf::from("/usr/bin/python3")),
            _ => panic!("expected Symlink"),
        }
    }
}
