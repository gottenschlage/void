//! Void's persisted workspace hierarchy.
//!
//! A workspace owns repositories. Each repository owns Void-managed branches,
//! and each branch has one dedicated Git worktree. Persistence records identity,
//! ordering, allocation, and archival; focused helpers perform Git validation and
//! managed-worktree creation.

mod paths;
mod persistence;
mod repository;

pub use paths::VoidPaths;
pub use persistence::{
    Branch, BranchId, NewBranch, NewRepository, Repository, RepositoryId, Workspace, WorkspaceDb,
    WorkspaceId,
};
pub use repository::{
    GitRepositoryLocation, create_git_worktree, inspect_git_repository, local_git_branches,
    validate_git_branch_request,
};
