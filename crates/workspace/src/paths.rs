use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

const APP_DIRECTORY_NAME: &str = "Void";
const DATABASE_FILE_NAME: &str = "void.db";
const WORKTREES_DIRECTORY_NAME: &str = "worktrees";

/// Filesystem locations owned by Void.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoidPaths {
    data_dir: PathBuf,
}

impl VoidPaths {
    /// Resolves Void's directory beneath the platform's standard local data directory.
    pub fn discover() -> Result<Self> {
        let data_dir = dirs::data_local_dir()
            .context("the operating system did not provide a local application-data directory")?
            .join(APP_DIRECTORY_NAME);
        Ok(Self { data_dir })
    }

    /// Creates a path set rooted at `data_dir`.
    ///
    /// This is useful for tests and explicit application-data overrides.
    pub fn from_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn database(&self) -> PathBuf {
        self.data_dir.join(DATABASE_FILE_NAME)
    }

    pub fn worktrees(&self) -> PathBuf {
        self.data_dir.join(WORKTREES_DIRECTORY_NAME)
    }

    /// Returns the worktree path for a repository and allocated branch name.
    ///
    /// Git branch separators are flattened so a branch such as `feature/auth`
    /// occupies one directory named `feature-auth`.
    pub fn branch_worktree(&self, repository_name: &str, branch_name: &str) -> Result<PathBuf> {
        validate_path_component(repository_name, "repository name")?;
        let directory_name = flatten_branch_name(branch_name)?;
        Ok(self.worktrees().join(repository_name).join(directory_name))
    }
}

pub(crate) fn validate_repository_name(value: &str) -> Result<()> {
    validate_path_component(value, "repository name")
}

fn validate_path_component(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        bail!("{label} must be one non-empty path component");
    }
    Ok(())
}

fn flatten_branch_name(branch_name: &str) -> Result<String> {
    if branch_name.is_empty()
        || branch_name.starts_with('/')
        || branch_name.ends_with('/')
        || branch_name.contains("//")
        || branch_name.contains('\\')
        || branch_name
            .split('/')
            .any(|part| part == "." || part == "..")
    {
        bail!("branch name cannot be represented as a safe worktree directory");
    }

    Ok(branch_name.replace('/', "-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_void_application_data_layout() {
        let paths = VoidPaths::from_data_dir("/data/Void");

        assert_eq!(paths.database(), Path::new("/data/Void/void.db"));
        assert_eq!(paths.worktrees(), Path::new("/data/Void/worktrees"));
        assert_eq!(
            paths.branch_worktree("zed", "feature/auth").unwrap(),
            Path::new("/data/Void/worktrees/zed/feature-auth")
        );
    }

    #[test]
    fn rejects_names_that_can_escape_the_layout() {
        let paths = VoidPaths::from_data_dir("/data/Void");

        assert!(paths.branch_worktree("../repo", "feature").is_err());
        assert!(paths.branch_worktree("repo", "../feature").is_err());
        assert!(paths.branch_worktree("repo", "feature//auth").is_err());
    }
}
