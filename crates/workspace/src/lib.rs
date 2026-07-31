#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

//! Void's workspace model, persistence, Git lifecycle, and GPUI surface.
//!
//! A workspace owns repositories. Each repository owns Void-managed branches,
//! and each branch has one dedicated Git worktree. Persistence records identity,
//! ordering, allocation, and archival; focused helpers perform Git validation and
//! managed-worktree creation. [`WorkspaceView`] projects that state and owns the
//! terminal, live-diff, dialog, and task resources tied to it.

mod git;
mod model;
mod paths;
mod persistence;
mod view;

pub use git::{
    DiffStat, GitDeleteMode, GitRepositoryLocation, GitWatchPaths, ManagedWorktreeError,
    RemovedManagedWorktree, ValidatedManagedWorktree, WorktreeProvenance, WorktreeProvenanceCheck,
    create_git_worktree, delete_managed_branch, git_watch_paths, head_to_worktree_diff_stat,
    inspect_git_repository, local_git_branches, remove_managed_worktree,
    validate_git_branch_request, validate_managed_worktree,
};
pub use model::WorkspaceModel;
pub use paths::VoidPaths;
pub use persistence::{
    Branch, BranchId, NewBranch, NewRepository, Repository, RepositoryId, Workspace, WorkspaceDb,
    WorkspaceId,
};
pub use view::{WorkspaceView, init};
