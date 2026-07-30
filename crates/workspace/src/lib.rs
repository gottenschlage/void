//! Void's persisted workspace hierarchy.
//!
//! A workspace owns repositories. Each repository owns Void-managed branches,
//! and each branch has one dedicated Git worktree. Git operations remain outside
//! this crate; persistence records identity, ordering, allocation, and archival.

mod paths;
mod persistence;

pub use paths::VoidPaths;
pub use persistence::{
    Branch, BranchId, NewBranch, NewRepository, Repository, RepositoryId, Workspace, WorkspaceDb,
    WorkspaceId,
};
