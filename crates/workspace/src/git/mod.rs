//! Focused Git process boundaries for repositories, worktrees, diffs, and observation.

mod diff;
mod live_diff;
mod repository;
mod worktree;

pub use diff::{DiffStat, GitWatchPaths, git_watch_paths, head_to_worktree_diff_stat};
pub use repository::{
    GitRepositoryLocation, inspect_git_repository, local_git_branches, validate_git_branch_request,
};
pub use worktree::{
    GitDeleteMode, ManagedWorktreeError, RemovedManagedWorktree, ValidatedManagedWorktree,
    WorktreeProvenance, WorktreeProvenanceCheck, create_git_worktree, delete_managed_branch,
    remove_managed_worktree, validate_managed_worktree,
};

use std::{
    path::Path,
    process::{Command, Output},
};

use anyhow::{Context as _, Result, bail};

pub(super) fn git<const N: usize>(repository_path: &Path, args: [&str; N]) -> Result<Output> {
    Command::new("git")
        .arg("-C")
        .arg(repository_path)
        .args(args)
        .output()
        .context("could not run Git; make sure git is installed")
}

pub(super) fn ensure_git_success(output: Output, context: &str) -> Result<Output> {
    if output.status.success() {
        return Ok(output);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim();
    if message.is_empty() {
        bail!("{context}");
    }
    bail!("{context}: {message}")
}

#[cfg(test)]
mod tests;

pub(crate) use live_diff::RepositoryLiveDiff;
