//! Typed domain model for virtual filesystem state, environment, layers, and provenance.
//!
//! This module is the single source of truth for the data structures that flow
//! between the parser, engine, and REPL. All engine state is expressed through
//! the types defined here.
//!
//! # Module layout
//!
//! | Module | Contents |
//! |--------|----------|
//! | `provenance` | `ProvenanceSource`, `MountInfo`, `Provenance` |
//! | `warning` | `Warning` enum for non-fatal diagnostics |
//! | `instruction` | `CopySource`, `Instruction` enum |
//! | `fs` | `FileNode`, `DirNode`, `SymlinkNode`, `FsNode`, `VirtualFs` |
//! | `state` | `InstalledRegistry`, `HistoryEntry`, `LayerSummary`, `PreviewState` |

pub mod fs;
pub mod instruction;
pub mod provenance;
pub mod state;
pub mod warning;

pub use fs::{FsNode, VirtualFs};
pub use instruction::Instruction;
pub use provenance::Provenance;
pub use state::PreviewState;
pub use warning::Warning;
