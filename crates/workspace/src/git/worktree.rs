use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::UNIX_EPOCH,
};

use anyhow::{Context as _, Result};
use thiserror::Error;

use super::{ensure_git_success, git, validate_git_branch_request};

/// Filesystem identity recorded for a worktree created or explicitly adopted by Void.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorktreeProvenance {
    pub git_dir: PathBuf,
    pub git_dir_created_at_ns: i64,
}

/// Provenance requirement applied while validating a managed worktree.
#[derive(Clone, Copy, Debug)]
pub enum WorktreeProvenanceCheck<'a> {
    /// Capture the live identity for a newly created or explicitly adopted legacy worktree.
    CaptureCurrent,
    /// Require the live identity to match the immutable database record.
    Recorded(&'a WorktreeProvenance),
}

/// A managed worktree whose repository, path, branch, registration, and provenance were checked.
#[derive(Clone, Debug)]
pub struct ValidatedManagedWorktree {
    repository_path: PathBuf,
    worktree_path: PathBuf,
    branch_name: String,
    provenance: WorktreeProvenance,
    registered: bool,
}

impl ValidatedManagedWorktree {
    pub fn provenance(&self) -> &WorktreeProvenance {
        &self.provenance
    }
}

/// Whether a destructive Git step may discard dirty or unmerged state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitDeleteMode {
    Safe,
    Force,
}

/// A validated branch whose worktree has been removed or was already absent.
#[derive(Clone, Debug)]
pub struct RemovedManagedWorktree {
    repository_path: PathBuf,
    branch_name: String,
}

/// Actionable failures from validating or deleting a Void-managed worktree.
#[derive(Debug, Error)]
pub enum ManagedWorktreeError {
    #[error("refusing to delete the worktree: {0}")]
    Refused(String),
    #[error("worktree contains modified or untracked files")]
    DirtyWorktree,
    #[error("branch {branch_name:?} is not fully merged")]
    UnmergedBranch { branch_name: String },
    #[error("{operation}: {message}")]
    Git {
        operation: &'static str,
        message: String,
    },
    #[error("could not inspect worktree metadata at {path}: {source}")]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
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

/// Validates that an exact branch/worktree pair is registered to the selected repository.
///
/// [`WorktreeProvenanceCheck::CaptureCurrent`] is only for new worktrees or legacy rows
/// after the caller has obtained explicit confirmation from the user.
pub fn validate_managed_worktree(
    repository_path: &Path,
    managed_worktrees_root: &Path,
    worktree_path: &Path,
    branch_name: &str,
    provenance_check: WorktreeProvenanceCheck<'_>,
) -> Result<ValidatedManagedWorktree, ManagedWorktreeError> {
    let repository_path = canonical_path(repository_path, "repository path")?;
    let repository_root = git_path(&repository_path, ["rev-parse", "--show-toplevel"])?;
    if canonical_path(&repository_root, "Git repository root")? != repository_path {
        return Err(ManagedWorktreeError::Refused(format!(
            "{} is not the selected Git repository root",
            repository_path.display()
        )));
    }

    let expected_worktree_path = if worktree_path.exists() {
        canonical_path(worktree_path, "worktree path")?
    } else if let (Some(parent), Some(name)) = (worktree_path.parent(), worktree_path.file_name()) {
        canonical_path(parent, "worktree parent directory")?.join(name)
    } else {
        worktree_path.to_path_buf()
    };
    let managed_worktrees_root = if managed_worktrees_root.exists() {
        canonical_path(managed_worktrees_root, "managed worktrees directory")?
    } else {
        managed_worktrees_root.to_path_buf()
    };
    if !expected_worktree_path.starts_with(&managed_worktrees_root) {
        return Err(ManagedWorktreeError::Refused(format!(
            "{} is outside Void's managed worktrees directory {}",
            expected_worktree_path.display(),
            managed_worktrees_root.display()
        )));
    }
    let registrations = worktree_registrations(&repository_path)?;
    let registration = registrations
        .iter()
        .find(|registration| paths_match(&registration.path, &expected_worktree_path));

    let Some(registration) = registration else {
        if worktree_path.exists() {
            return Err(ManagedWorktreeError::Refused(format!(
                "{} is not registered as a Git worktree for {}",
                worktree_path.display(),
                repository_path.display()
            )));
        }
        let WorktreeProvenanceCheck::Recorded(provenance) = provenance_check else {
            return Err(ManagedWorktreeError::Refused(
                "the legacy worktree is missing and cannot be safely adopted".into(),
            ));
        };
        if provenance.git_dir.exists() {
            return Err(ManagedWorktreeError::Refused(format!(
                "Git still has metadata for the missing worktree at {}",
                provenance.git_dir.display()
            )));
        }
        if registrations
            .iter()
            .any(|registration| registration.branch.as_deref() == Some(branch_name))
        {
            return Err(ManagedWorktreeError::Refused(format!(
                "branch {branch_name:?} is checked out in a different worktree"
            )));
        }
        return Ok(ValidatedManagedWorktree {
            repository_path,
            worktree_path: worktree_path.to_path_buf(),
            branch_name: branch_name.to_owned(),
            provenance: provenance.clone(),
            registered: false,
        });
    };

    if registration.branch.as_deref() != Some(branch_name) {
        return Err(ManagedWorktreeError::Refused(format!(
            "{} is registered for branch {:?}, not {branch_name:?}",
            worktree_path.display(),
            registration.branch.as_deref().unwrap_or("a detached HEAD")
        )));
    }

    let provenance = worktree_provenance(&repository_path, worktree_path, branch_name)?;
    if let WorktreeProvenanceCheck::Recorded(recorded) = provenance_check
        && recorded != &provenance
    {
        return Err(ManagedWorktreeError::Refused(format!(
            "{} is not the worktree Void recorded; it may have been recreated",
            worktree_path.display()
        )));
    }

    Ok(ValidatedManagedWorktree {
        repository_path,
        worktree_path: expected_worktree_path,
        branch_name: branch_name.to_owned(),
        provenance,
        registered: true,
    })
}

/// Removes a validated worktree, preserving dirty files unless `force` is true.
pub fn remove_managed_worktree(
    worktree: &ValidatedManagedWorktree,
    mode: GitDeleteMode,
) -> Result<RemovedManagedWorktree, ManagedWorktreeError> {
    if worktree.registered {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(&worktree.repository_path)
            .args(["worktree", "remove"]);
        if mode == GitDeleteMode::Force {
            command.arg("--force");
        }
        let output = command
            .arg(&worktree.worktree_path)
            .output()
            .map_err(|error| ManagedWorktreeError::Git {
                operation: "could not run git worktree remove",
                message: error.to_string(),
            })?;
        if !output.status.success() {
            let message = git_error_message(&output);
            let normalized = message.to_lowercase();
            if mode == GitDeleteMode::Safe
                && normalized.contains("contains modified or untracked files")
                && normalized.contains("use --force to delete it")
            {
                return Err(ManagedWorktreeError::DirtyWorktree);
            }
            return Err(ManagedWorktreeError::Git {
                operation: "could not remove the Git worktree",
                message,
            });
        }
    }

    Ok(RemovedManagedWorktree {
        repository_path: worktree.repository_path.clone(),
        branch_name: worktree.branch_name.clone(),
    })
}

/// Deletes the local branch for a worktree that has already been removed.
pub fn delete_managed_branch(
    worktree: &RemovedManagedWorktree,
    mode: GitDeleteMode,
) -> Result<(), ManagedWorktreeError> {
    let full_ref = format!("refs/heads/{}", worktree.branch_name);
    let exists = git(
        &worktree.repository_path,
        ["show-ref", "--verify", "--quiet", &full_ref],
    )
    .map_err(|error| ManagedWorktreeError::Git {
        operation: "could not inspect the local branch",
        message: error.to_string(),
    })?;
    if exists.status.code() == Some(1) {
        return Ok(());
    }
    if !exists.status.success() {
        return Err(ManagedWorktreeError::Git {
            operation: "could not inspect the local branch",
            message: git_error_message(&exists),
        });
    }

    let flag = match mode {
        GitDeleteMode::Safe => "-d",
        GitDeleteMode::Force => "-D",
    };
    let output = git(
        &worktree.repository_path,
        ["branch", flag, "--", &worktree.branch_name],
    )
    .map_err(|error| ManagedWorktreeError::Git {
        operation: "could not run git branch",
        message: error.to_string(),
    })?;
    if output.status.success() {
        return Ok(());
    }

    let message = git_error_message(&output);
    if mode == GitDeleteMode::Safe && message.to_lowercase().contains("not fully merged") {
        return Err(ManagedWorktreeError::UnmergedBranch {
            branch_name: worktree.branch_name.clone(),
        });
    }
    Err(ManagedWorktreeError::Git {
        operation: "could not delete the local Git branch",
        message,
    })
}

fn worktree_provenance(
    repository_path: &Path,
    worktree_path: &Path,
    branch_name: &str,
) -> Result<WorktreeProvenance, ManagedWorktreeError> {
    let root = git_path(worktree_path, ["rev-parse", "--show-toplevel"])?;
    let root = canonical_path(&root, "worktree root")?;
    let expected_root = canonical_path(worktree_path, "worktree path")?;
    if root != expected_root {
        return Err(ManagedWorktreeError::Refused(format!(
            "{} does not resolve to its registered worktree root",
            worktree_path.display()
        )));
    }

    let actual_branch = git_text(worktree_path, ["symbolic-ref", "--short", "HEAD"])?;
    if actual_branch != branch_name {
        return Err(ManagedWorktreeError::Refused(format!(
            "{} has branch {actual_branch:?} checked out instead of {branch_name:?}",
            worktree_path.display()
        )));
    }

    let repository_common_dir = git_path(
        repository_path,
        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let worktree_common_dir = git_path(
        worktree_path,
        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    if canonical_path(&repository_common_dir, "repository Git directory")?
        != canonical_path(&worktree_common_dir, "worktree common Git directory")?
    {
        return Err(ManagedWorktreeError::Refused(format!(
            "{} belongs to a different Git repository",
            worktree_path.display()
        )));
    }

    let git_dir = git_path(
        worktree_path,
        ["rev-parse", "--path-format=absolute", "--git-dir"],
    )?;
    let git_dir = canonical_path(&git_dir, "worktree Git directory")?;
    let created_at = fs::metadata(&git_dir)
        .and_then(|metadata| metadata.created())
        .map_err(|source| ManagedWorktreeError::Metadata {
            path: git_dir.clone(),
            source,
        })?;
    let created_at_ns = created_at
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            ManagedWorktreeError::Refused(format!(
                "worktree metadata creation time is before the Unix epoch: {error}"
            ))
        })?
        .as_nanos();
    let git_dir_created_at_ns = i64::try_from(created_at_ns).map_err(|_| {
        ManagedWorktreeError::Refused("worktree metadata creation time is out of range".into())
    })?;

    Ok(WorktreeProvenance {
        git_dir,
        git_dir_created_at_ns,
    })
}

#[derive(Debug)]
struct WorktreeRegistration {
    path: PathBuf,
    branch: Option<String>,
}

fn worktree_registrations(
    repository_path: &Path,
) -> Result<Vec<WorktreeRegistration>, ManagedWorktreeError> {
    let output =
        git(repository_path, ["worktree", "list", "--porcelain", "-z"]).map_err(|error| {
            ManagedWorktreeError::Git {
                operation: "could not list Git worktrees",
                message: error.to_string(),
            }
        })?;
    if !output.status.success() {
        return Err(ManagedWorktreeError::Git {
            operation: "could not list Git worktrees",
            message: git_error_message(&output),
        });
    }
    let text = String::from_utf8(output.stdout).map_err(|error| ManagedWorktreeError::Git {
        operation: "could not list Git worktrees",
        message: format!("Git returned a path that is not valid UTF-8: {error}"),
    })?;

    text.split("\0\0")
        .filter(|record| !record.is_empty())
        .map(|record| {
            let mut path = None;
            let mut branch = None;
            for field in record.split('\0') {
                if let Some(value) = field.strip_prefix("worktree ") {
                    path = Some(PathBuf::from(value));
                } else if let Some(value) = field.strip_prefix("branch refs/heads/") {
                    branch = Some(value.to_owned());
                }
            }
            path.map(|path| WorktreeRegistration { path, branch })
                .ok_or_else(|| ManagedWorktreeError::Git {
                    operation: "could not parse Git worktree registration",
                    message: "a worktree record did not contain a path".into(),
                })
        })
        .collect()
}

fn paths_match(registered: &Path, expected: &Path) -> bool {
    if registered == expected {
        return true;
    }
    registered
        .canonicalize()
        .ok()
        .zip(expected.canonicalize().ok())
        .is_some_and(|(registered, expected)| registered == expected)
}

fn canonical_path(path: &Path, label: &'static str) -> Result<PathBuf, ManagedWorktreeError> {
    path.canonicalize().map_err(|error| {
        ManagedWorktreeError::Refused(format!(
            "could not resolve {label} {}: {error}",
            path.display()
        ))
    })
}

fn git_path<const N: usize>(
    directory: &Path,
    args: [&str; N],
) -> Result<PathBuf, ManagedWorktreeError> {
    git_text(directory, args).map(PathBuf::from)
}

fn git_text<const N: usize>(
    directory: &Path,
    args: [&str; N],
) -> Result<String, ManagedWorktreeError> {
    let output = git(directory, args).map_err(|error| ManagedWorktreeError::Git {
        operation: "could not run Git while validating the worktree",
        message: error.to_string(),
    })?;
    if !output.status.success() {
        return Err(ManagedWorktreeError::Git {
            operation: "could not validate the Git worktree",
            message: git_error_message(&output),
        });
    }
    String::from_utf8(output.stdout)
        .map(|text| text.trim().to_owned())
        .map_err(|error| ManagedWorktreeError::Git {
            operation: "could not validate the Git worktree",
            message: format!("Git returned text that is not valid UTF-8: {error}"),
        })
}

fn git_error_message(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        format!("Git exited with {}", output.status)
    } else {
        stderr
    }
}
