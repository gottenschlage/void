use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use smol::process::Command as AsyncCommand;

use super::{ensure_git_success, git};

/// Added and deleted lines in a Git diff.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiffStat {
    pub added: u32,
    pub deleted: u32,
}

/// Git directories whose changes can affect a managed worktree's diff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitWatchPaths {
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
}

/// Counts tracked staged and unstaged changes between `HEAD` and a worktree.
///
/// Binary and untracked files do not contribute line counts, matching
/// `git diff --numstat` and Zed's Git-panel diff statistic.
pub async fn head_to_worktree_diff_stat(worktree_path: &Path) -> Result<DiffStat> {
    let mut command = AsyncCommand::new("git");
    command
        .arg("-C")
        .arg(worktree_path)
        .args(["diff", "--numstat", "--no-renames", "HEAD", "--"])
        .kill_on_drop(true);
    let output = command
        .output()
        .await
        .context("could not run Git; make sure git is installed")?;
    let output = ensure_git_success(output, "could not calculate worktree diff")?;
    let stdout =
        String::from_utf8(output.stdout).context("Git returned diff output that is not UTF-8")?;
    Ok(parse_numstat(&stdout))
}

/// Resolves the worktree-specific and shared Git directories for file watching.
pub fn git_watch_paths(worktree_path: &Path) -> Result<GitWatchPaths> {
    Ok(GitWatchPaths {
        git_dir: git_path(worktree_path, "--absolute-git-dir")?,
        common_dir: git_path(worktree_path, "--git-common-dir")?,
    })
}

fn git_path(worktree_path: &Path, argument: &str) -> Result<PathBuf> {
    let output = git(
        worktree_path,
        ["rev-parse", "--path-format=absolute", argument],
    )?;
    let output = ensure_git_success(output, "could not resolve Git metadata")?;
    let path =
        String::from_utf8(output.stdout).context("Git returned a path that is not valid UTF-8")?;
    Ok(PathBuf::from(path.trim()))
}

pub(super) fn parse_numstat(output: &str) -> DiffStat {
    output.lines().fold(DiffStat::default(), |mut total, line| {
        let mut fields = line.splitn(3, '\t');
        let (Some(added), Some(deleted), Some(_path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return total;
        };
        let (Ok(added), Ok(deleted)) = (added.parse::<u32>(), deleted.parse::<u32>()) else {
            return total;
        };
        total.added = total.added.saturating_add(added);
        total.deleted = total.deleted.saturating_add(deleted);
        total
    })
}
