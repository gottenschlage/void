use super::diff::parse_numstat;
use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct TestRepository(std::path::PathBuf);

impl TestRepository {
    fn new() -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("void-repository-test-{}-{id}", std::process::id()));
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
fn deletes_a_clean_worktree_and_its_merged_branch_without_force() {
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
    let validated = validate_managed_worktree(
        &repository.0,
        std::env::temp_dir().as_path(),
        &worktree_path,
        "feature/deletable",
        WorktreeProvenanceCheck::CaptureCurrent,
    )
    .unwrap();
    let removed = remove_managed_worktree(&validated, GitDeleteMode::Safe).unwrap();
    delete_managed_branch(&removed, GitDeleteMode::Safe).unwrap();

    assert!(!worktree_path.exists());
    assert!(
        !local_git_branches(&repository.0)
            .unwrap()
            .contains(&"feature/deletable".to_owned())
    );
}

#[test]
fn refuses_dirty_worktree_until_force_is_explicit() {
    let repository = TestRepository::new();
    repository.create_initial_commit();
    let base_branch = local_git_branches(&repository.0).unwrap().remove(0);
    let worktree_path = repository.0.with_extension("dirty-worktree");
    create_git_worktree(&repository.0, "feature/dirty", &worktree_path, &base_branch).unwrap();
    fs::write(worktree_path.join("README.md"), "dirty\n").unwrap();
    fs::write(worktree_path.join("untracked.txt"), "untracked\n").unwrap();
    let validated = validate_managed_worktree(
        &repository.0,
        std::env::temp_dir().as_path(),
        &worktree_path,
        "feature/dirty",
        WorktreeProvenanceCheck::CaptureCurrent,
    )
    .unwrap();

    let error = remove_managed_worktree(&validated, GitDeleteMode::Safe).unwrap_err();
    assert!(matches!(error, ManagedWorktreeError::DirtyWorktree));
    assert!(worktree_path.exists());

    let removed = remove_managed_worktree(&validated, GitDeleteMode::Force).unwrap();
    delete_managed_branch(&removed, GitDeleteMode::Safe).unwrap();
}

#[test]
fn refuses_unmerged_branch_until_force_is_explicit() {
    let repository = TestRepository::new();
    repository.create_initial_commit();
    let base_branch = local_git_branches(&repository.0).unwrap().remove(0);
    let worktree_path = repository.0.with_extension("unmerged-worktree");
    create_git_worktree(
        &repository.0,
        "feature/unmerged",
        &worktree_path,
        &base_branch,
    )
    .unwrap();
    fs::write(worktree_path.join("feature.txt"), "new commit\n").unwrap();
    for args in [
        ["add", "feature.txt"].as_slice(),
        ["commit", "--quiet", "-m", "Unmerged commit"].as_slice(),
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&worktree_path)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
    let validated = validate_managed_worktree(
        &repository.0,
        std::env::temp_dir().as_path(),
        &worktree_path,
        "feature/unmerged",
        WorktreeProvenanceCheck::CaptureCurrent,
    )
    .unwrap();
    let removed = remove_managed_worktree(&validated, GitDeleteMode::Safe).unwrap();

    let error = delete_managed_branch(&removed, GitDeleteMode::Safe).unwrap_err();
    assert!(matches!(error, ManagedWorktreeError::UnmergedBranch { .. }));
    assert!(
        local_git_branches(&repository.0)
            .unwrap()
            .contains(&"feature/unmerged".to_owned())
    );

    delete_managed_branch(&removed, GitDeleteMode::Force).unwrap();
}

#[test]
fn rejects_a_registered_worktree_outside_the_managed_directory() {
    let repository = TestRepository::new();
    repository.create_initial_commit();
    let base_branch = local_git_branches(&repository.0).unwrap().remove(0);
    let worktree_path = repository.0.with_extension("external-worktree");
    create_git_worktree(
        &repository.0,
        "feature/external",
        &worktree_path,
        &base_branch,
    )
    .unwrap();
    let unrelated_root = repository.0.join("managed");
    fs::create_dir(&unrelated_root).unwrap();

    let error = validate_managed_worktree(
        &repository.0,
        &unrelated_root,
        &worktree_path,
        "feature/external",
        WorktreeProvenanceCheck::CaptureCurrent,
    )
    .unwrap_err();

    assert!(matches!(error, ManagedWorktreeError::Refused(_)));
    fs::remove_dir_all(worktree_path).ok();
}

#[test]
fn rejects_a_worktree_registered_for_a_different_branch() {
    let repository = TestRepository::new();
    repository.create_initial_commit();
    let base_branch = local_git_branches(&repository.0).unwrap().remove(0);
    let worktree_path = repository.0.with_extension("mismatched-worktree");
    create_git_worktree(
        &repository.0,
        "feature/actual",
        &worktree_path,
        &base_branch,
    )
    .unwrap();

    let error = validate_managed_worktree(
        &repository.0,
        std::env::temp_dir().as_path(),
        &worktree_path,
        "feature/expected",
        WorktreeProvenanceCheck::CaptureCurrent,
    )
    .unwrap_err();

    assert!(matches!(error, ManagedWorktreeError::Refused(_)));
    fs::remove_dir_all(worktree_path).ok();
}

#[test]
fn rejects_recreated_worktree_when_provenance_differs() {
    let repository = TestRepository::new();
    repository.create_initial_commit();
    let base_branch = local_git_branches(&repository.0).unwrap().remove(0);
    let worktree_path = repository.0.with_extension("provenance-worktree");
    create_git_worktree(
        &repository.0,
        "feature/provenance",
        &worktree_path,
        &base_branch,
    )
    .unwrap();
    let mut recorded = validate_managed_worktree(
        &repository.0,
        std::env::temp_dir().as_path(),
        &worktree_path,
        "feature/provenance",
        WorktreeProvenanceCheck::CaptureCurrent,
    )
    .unwrap()
    .provenance()
    .clone();
    recorded.git_dir_created_at_ns -= 1;

    let error = validate_managed_worktree(
        &repository.0,
        std::env::temp_dir().as_path(),
        &worktree_path,
        "feature/provenance",
        WorktreeProvenanceCheck::Recorded(&recorded),
    )
    .unwrap_err();

    assert!(matches!(error, ManagedWorktreeError::Refused(_)));
    fs::remove_dir_all(worktree_path).ok();
}

#[test]
fn resumes_after_the_worktree_was_already_removed() {
    let repository = TestRepository::new();
    repository.create_initial_commit();
    let base_branch = local_git_branches(&repository.0).unwrap().remove(0);
    let worktree_path = repository.0.with_extension("partial-worktree");
    create_git_worktree(
        &repository.0,
        "feature/partial",
        &worktree_path,
        &base_branch,
    )
    .unwrap();
    let validated = validate_managed_worktree(
        &repository.0,
        std::env::temp_dir().as_path(),
        &worktree_path,
        "feature/partial",
        WorktreeProvenanceCheck::CaptureCurrent,
    )
    .unwrap();
    let provenance = validated.provenance().clone();
    remove_managed_worktree(&validated, GitDeleteMode::Safe).unwrap();

    let resumed = validate_managed_worktree(
        &repository.0,
        std::env::temp_dir().as_path(),
        &worktree_path,
        "feature/partial",
        WorktreeProvenanceCheck::Recorded(&provenance),
    )
    .unwrap();
    let removed = remove_managed_worktree(&resumed, GitDeleteMode::Safe).unwrap();
    delete_managed_branch(&removed, GitDeleteMode::Safe).unwrap();
}
