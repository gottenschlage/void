use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, Result, bail};

use super::{ensure_git_success, git};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitRepositoryLocation {
    pub name: String,
    pub path: PathBuf,
}

/// Resolves a selected directory to a canonical Git worktree root.
///
/// Void accepts normal repositories and linked worktrees, but not bare
/// repositories or directories nested below a worktree root.
pub fn inspect_git_repository(path: &Path) -> Result<GitRepositoryLocation> {
    let path = path
        .canonicalize()
        .with_context(|| format!("could not resolve {}", path.display()))?;
    if !path.is_dir() {
        bail!("select a repository directory");
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(&path)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("could not run Git; make sure git is installed")?;
    if !output.status.success() {
        bail!("the selected directory is not a Git repository");
    }

    let root = String::from_utf8(output.stdout)
        .context("Git returned a repository path that is not valid UTF-8")?;
    let root = Path::new(root.trim())
        .canonicalize()
        .context("could not resolve the Git repository root")?;
    if root != path {
        bail!("select the Git repository's root directory");
    }

    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .context("the repository directory has no valid UTF-8 name")?
        .to_owned();

    Ok(GitRepositoryLocation { name, path })
}

/// Lists local branches with the checked-out branch first.
pub fn local_git_branches(repository_path: &Path) -> Result<Vec<String>> {
    let output = git(
        repository_path,
        ["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )?;
    ensure_git_success(output, "could not list local branches").and_then(|output| {
        let stdout = String::from_utf8(output.stdout)
            .context("Git returned a branch name that is not valid UTF-8")?;
        let mut branches = stdout
            .lines()
            .filter(|branch| !branch.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();

        if let Ok(output) = git(repository_path, ["symbolic-ref", "--short", "HEAD"])
            && output.status.success()
        {
            let current = String::from_utf8(output.stdout)
                .context("Git returned a current branch that is not valid UTF-8")?;
            let current = current.trim();
            if let Some(index) = branches.iter().position(|branch| branch == current) {
                branches.swap(0, index);
            }
        }

        if branches.is_empty() {
            bail!("the repository has no local branches; create an initial commit first");
        }
        Ok(branches)
    })
}

/// Validates a requested branch name and base ref without changing the repository.
pub fn validate_git_branch_request(
    repository_path: &Path,
    branch_name: &str,
    base_ref: &str,
) -> Result<()> {
    ensure_git_success(
        git(
            repository_path,
            ["check-ref-format", "--branch", branch_name],
        )?,
        "the branch name is not valid",
    )?;
    ensure_git_success(
        git(
            repository_path,
            ["rev-parse", "--verify", &format!("{base_ref}^{{commit}}")],
        )?,
        "the base branch does not resolve to a commit",
    )?;

    let full_ref = format!("refs/heads/{branch_name}");
    let output = git(
        repository_path,
        ["show-ref", "--verify", "--quiet", &full_ref],
    )?;
    if output.status.success() {
        bail!("a Git branch named {branch_name:?} already exists");
    }
    if output.status.code() != Some(1) {
        ensure_git_success(output, "could not check the branch name")?;
    }
    Ok(())
}
