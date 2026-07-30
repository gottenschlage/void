use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

use anyhow::{Context as _, Result, bail};

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
        assert!(
            local_git_branches(&repository.0)
                .unwrap()
                .contains(&"feature/test-worktree".to_owned())
        );
        fs::remove_dir_all(worktree_path).ok();
    }
}
