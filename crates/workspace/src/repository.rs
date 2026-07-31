use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

use anyhow::{Context as _, Result, bail};
use smol::process::Command as AsyncCommand;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitRepositoryLocation {
    pub name: String,
    pub path: PathBuf,
}

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

fn parse_numstat(output: &str) -> DiffStat {
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

/// Creates a Git branch and checks it out in a dedicated worktree.
pub fn create_git_worktree(
    repository_path: &Path,
    branch_name: &str,
    worktree_path: &Path,
    base_ref: &str,
) -> Result<()> {
    validate_git_branch_request(repository_path, branch_name, base_ref)?;

    if let Some(parent) = worktree_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(repository_path)
        .args(["worktree", "add", "-b", branch_name])
        .arg(worktree_path)
        .arg(base_ref)
        .output()
        .context("could not run Git; make sure git is installed")?;
    ensure_git_success(output, "could not create the Git worktree")?;
    Ok(())
}

/// Removes a branch's worktree and deletes the local Git branch.
///
/// The worktree is removed first: Git refuses to delete a branch that is
/// still checked out in a worktree.
pub fn delete_git_worktree(
    repository_path: &Path,
    worktree_path: &Path,
    branch_name: &str,
) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository_path)
        .args(["worktree", "remove", "--force"])
        .arg(worktree_path)
        .output()
        .context("could not run Git; make sure git is installed")?;
    ensure_git_success(output, "could not remove the Git worktree")?;

    let output = git(repository_path, ["branch", "-D", branch_name])?;
    ensure_git_success(output, "could not delete the Git branch")?;
    Ok(())
}

fn git<const N: usize>(repository_path: &Path, args: [&str; N]) -> Result<Output> {
    Command::new("git")
        .arg("-C")
        .arg(repository_path)
        .args(args)
        .output()
        .context("could not run Git; make sure git is installed")
}

fn ensure_git_success(output: Output, context: &str) -> Result<Output> {
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
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    struct TestRepository(std::path::PathBuf);

    impl TestRepository {
        fn new() -> Self {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("void-repository-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            let status = Command::new("git")
                .args(["init", "--quiet"])
                .arg(&path)
                .status()
                .unwrap();
            assert!(status.success());
            Self(path)
        }

        fn create_initial_commit(&self) {
            fs::write(self.0.join("README.md"), "test repository\n").unwrap();
            for args in [
                ["config", "user.name", "Void Tests"].as_slice(),
                ["config", "user.email", "void-tests@example.invalid"].as_slice(),
                ["add", "README.md"].as_slice(),
                ["commit", "--quiet", "-m", "Initial commit"].as_slice(),
            ] {
                assert!(
                    Command::new("git")
                        .arg("-C")
                        .arg(&self.0)
                        .args(args)
                        .status()
                        .unwrap()
                        .success()
                );
            }
        }
    }

    impl Drop for TestRepository {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    #[test]
    fn accepts_only_the_selected_git_worktree_root() {
        let repository = TestRepository::new();
        let location = inspect_git_repository(&repository.0).unwrap();
        assert_eq!(location.path, repository.0.canonicalize().unwrap());
        assert_eq!(
            location.name,
            repository.0.file_name().unwrap().to_string_lossy()
        );

        let nested = repository.0.join("nested");
        fs::create_dir(&nested).unwrap();
        assert!(
            inspect_git_repository(&nested)
                .unwrap_err()
                .to_string()
                .contains("root directory")
        );
    }

    #[test]
    fn parses_text_numstat_and_ignores_binary_entries() {
        assert_eq!(
            parse_numstat("12\t3\tsrc/main.rs\n-\t-\timage.png\n4\t9\tREADME.md\n"),
            DiffStat {
                added: 16,
                deleted: 12,
            }
        );
    }

    #[test]
    fn counts_staged_and_unstaged_changes_from_head() {
        let repository = TestRepository::new();
        repository.create_initial_commit();

        fs::write(repository.0.join("README.md"), "changed\nsecond line\n").unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repository.0)
                .args(["add", "README.md"])
                .status()
                .unwrap()
                .success()
        );
        fs::write(
            repository.0.join("README.md"),
            "changed again\nsecond line\nthird line\n",
        )
        .unwrap();

        assert_eq!(
            pollster::block_on(head_to_worktree_diff_stat(&repository.0)).unwrap(),
            DiffStat {
                added: 3,
                deleted: 1,
            }
        );
    }

    #[test]
    fn excludes_untracked_files() {
        let repository = TestRepository::new();
        repository.create_initial_commit();
        fs::write(repository.0.join("untracked.txt"), "not counted\n").unwrap();

        assert_eq!(
            pollster::block_on(head_to_worktree_diff_stat(&repository.0)).unwrap(),
            DiffStat::default()
        );
    }

    #[test]
    fn counts_staged_new_files_and_resets_after_commit() {
        let repository = TestRepository::new();
        repository.create_initial_commit();
        fs::write(repository.0.join("new.txt"), "one\ntwo\n").unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repository.0)
                .args(["add", "new.txt"])
                .status()
                .unwrap()
                .success()
        );

        assert_eq!(
            pollster::block_on(head_to_worktree_diff_stat(&repository.0)).unwrap(),
            DiffStat {
                added: 2,
                deleted: 0,
            }
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repository.0)
                .args(["commit", "--quiet", "-m", "Add new file"])
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(
            pollster::block_on(head_to_worktree_diff_stat(&repository.0)).unwrap(),
            DiffStat::default()
        );
    }

    #[test]
    fn excludes_binary_line_counts() {
        let repository = TestRepository::new();
        repository.create_initial_commit();
        fs::write(repository.0.join("binary.dat"), b"before\0after").unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repository.0)
                .args(["add", "binary.dat"])
                .status()
                .unwrap()
                .success()
        );

        assert_eq!(
            pollster::block_on(head_to_worktree_diff_stat(&repository.0)).unwrap(),
            DiffStat::default()
        );
    }

    #[test]
    fn rejects_a_directory_without_git_metadata() {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "void-non-repository-test-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        let result = inspect_git_repository(&path);
        fs::remove_dir_all(path).ok();

        assert!(result.is_err());
    }

    #[test]
    fn lists_local_branches_and_creates_a_worktree() {
        let repository = TestRepository::new();
        repository.create_initial_commit();
        let base_branch = local_git_branches(&repository.0).unwrap().remove(0);
        let worktree_path = repository.0.with_extension("managed-worktree");

        create_git_worktree(
            &repository.0,
            "feature/test-worktree",
            &worktree_path,
            &base_branch,
        )
        .unwrap();

        assert!(worktree_path.join(".git").is_file());
        let watch_paths = git_watch_paths(&worktree_path).unwrap();
        assert!(watch_paths.git_dir.is_dir());
        assert!(watch_paths.common_dir.is_dir());
        assert_ne!(watch_paths.git_dir, watch_paths.common_dir);
        assert!(
            local_git_branches(&repository.0)
                .unwrap()
                .contains(&"feature/test-worktree".to_owned())
        );
        fs::remove_dir_all(worktree_path).ok();
    }

    #[test]
    fn deletes_a_worktree_and_its_branch() {
        let repository = TestRepository::new();
        repository.create_initial_commit();
        let base_branch = local_git_branches(&repository.0).unwrap().remove(0);
        let worktree_path = repository.0.with_extension("deletable-worktree");

        create_git_worktree(
            &repository.0,
            "feature/deletable",
            &worktree_path,
            &base_branch,
        )
        .unwrap();

        delete_git_worktree(&repository.0, &worktree_path, "feature/deletable").unwrap();

        assert!(!worktree_path.exists());
        assert!(
            !local_git_branches(&repository.0)
                .unwrap()
                .contains(&"feature/deletable".to_owned())
        );
    }
}
