use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use sqlez_macros::sql;

use crate::{git::WorktreeProvenance, paths::VoidPaths};

use super::{Branch, BranchId, NewBranch, RepositoryId, WorkspaceDb};

impl WorkspaceDb {
    /// Reserves the next branch number, unique branch name, and worktree path.
    ///
    /// Archived names and paths remain reserved. If the requested name or its
    /// flattened path was used before, `-2`, `-3`, and so on are tried.
    pub async fn reserve_branch(&self, branch: NewBranch, paths: &VoidPaths) -> Result<Branch> {
        if branch.position < 0 {
            bail!("branch position cannot be negative");
        }
        if branch.base_ref.trim().is_empty() {
            bail!("branch base ref cannot be empty");
        }

        let paths = paths.clone();
        self.connection
            .write(move |connection| {
                connection.with_savepoint("reserve_branch", || {
                    let (repository_name, sequence) =
                        connection.select_row_bound::<RepositoryId, (String, i64)>(sql!(
                            SELECT name, sequence
                            FROM repositories
                            WHERE id = ? AND archived_at IS NULL
                        ))?(branch.repository_id)?
                        .context("active repository was not found")?;

                    let number = sequence
                        .checked_add(1)
                        .context("repository branch sequence is exhausted")?;
                    let (name, path) = available_branch_name_and_path(
                        connection,
                        branch.repository_id,
                        &repository_name,
                        &branch.requested_name,
                        &paths,
                    )?;

                    connection.exec_bound::<(i64, RepositoryId)>(sql!(
                        UPDATE repositories
                        SET sequence = ?, updated_at = CURRENT_TIMESTAMP
                        WHERE id = ?
                    ))?((number, branch.repository_id))?;

                    let id = connection
                        .select_row_bound::<
                            (RepositoryId, i64, String, PathBuf, String, i64, bool),
                            BranchId,
                        >(
                            sql!(
                                INSERT INTO branches(
                                    repository_id,
                                    number,
                                    name,
                                    path,
                                    base_ref,
                                    position,
                                    is_pinned
                                )
                                VALUES (?, ?, ?, ?, ?, ?, ?)
                                RETURNING id
                            ),
                        )?((
                            branch.repository_id,
                            number,
                            name.clone(),
                            path.clone(),
                            branch.base_ref.clone(),
                            branch.position,
                            branch.is_pinned,
                        ))?
                        .context("branch insert did not return an id")?;

                    Ok(Branch {
                        id,
                        repository_id: branch.repository_id,
                        number,
                        name,
                        path,
                        base_ref: branch.base_ref,
                        position: branch.position,
                        is_pinned: branch.is_pinned,
                        archived_at: None,
                        worktree_provenance: None,
                    })
                })
            })
            .await
            .context("failed to reserve branch")
    }

    /// Records the Git metadata identity for a newly created or explicitly adopted worktree.
    pub async fn record_worktree_provenance(
        &self,
        branch_id: BranchId,
        provenance: WorktreeProvenance,
    ) -> Result<()> {
        self.connection
            .write(move |connection| {
                connection.with_savepoint("record_worktree_provenance", || {
                    let columns = connection
                        .select_row_bound::<BranchId, (Option<PathBuf>, Option<i64>)>(sql!(
                            SELECT worktree_git_dir, worktree_git_dir_created_at_ns
                            FROM branches
                            WHERE id = ?
                        ))?(branch_id)?
                    .context("branch was not found")?;
                    if let Some(recorded) = worktree_provenance(columns)? {
                        if recorded == provenance {
                            return Ok(());
                        }
                        bail!("refusing to replace recorded worktree provenance");
                    }

                    connection.exec_bound::<(PathBuf, i64, BranchId)>(sql!(
                        UPDATE branches
                        SET worktree_git_dir = ?,
                            worktree_git_dir_created_at_ns = ?,
                            updated_at = CURRENT_TIMESTAMP
                        WHERE id = ?
                    ))?((
                        provenance.git_dir,
                        provenance.git_dir_created_at_ns,
                        branch_id,
                    ))
                })
            })
            .await
            .context("failed to record worktree provenance")
    }

    pub async fn archive_branch(&self, branch_id: BranchId) -> Result<()> {
        self.connection
            .write(move |connection| {
                connection.exec_bound::<BranchId>(sql!(
                    UPDATE branches
                    SET archived_at = CURRENT_TIMESTAMP,
                        updated_at = CURRENT_TIMESTAMP,
                        is_pinned = FALSE
                    WHERE id = ? AND archived_at IS NULL
                ))?(branch_id)
            })
            .await
            .context("failed to archive branch")
    }

    /// Permanently removes a branch's record.
    ///
    /// Callers must remove the branch's Git worktree and local branch first;
    /// this does not touch the filesystem or Git state.
    pub async fn delete_branch(&self, branch_id: BranchId) -> Result<()> {
        self.connection
            .write(move |connection| {
                connection.exec_bound::<BranchId>(sql!(
                    DELETE FROM branches WHERE id = ?
                ))?(branch_id)
            })
            .await
            .context("failed to delete branch")
    }

    /// Persists the visible branch order within one repository.
    pub async fn reorder_branches(
        &self,
        repository_id: RepositoryId,
        branch_ids: Vec<BranchId>,
    ) -> Result<()> {
        self.connection
            .write(move |connection| {
                connection.with_savepoint("reorder_branches", || {
                    for (position, branch_id) in branch_ids.into_iter().enumerate() {
                        connection.exec_bound::<(i64, BranchId, RepositoryId)>(sql!(
                            UPDATE branches
                            SET position = ?, updated_at = CURRENT_TIMESTAMP
                            WHERE id = ? AND repository_id = ? AND archived_at IS NULL
                        ))?((position as i64, branch_id, repository_id))?;
                    }
                    Ok(())
                })
            })
            .await
            .context("failed to reorder branches")
    }

    pub fn branches(&self, repository_id: RepositoryId) -> Result<Vec<Branch>> {
        let rows = self.connection.select_bound::<RepositoryId, (
            BranchId,
            RepositoryId,
            i64,
            String,
            PathBuf,
            String,
            i64,
            bool,
            Option<String>,
            (Option<PathBuf>, Option<i64>),
        )>(sql!(
            SELECT
                id,
                repository_id,
                number,
                name,
                path,
                base_ref,
                position,
                is_pinned,
                archived_at,
                worktree_git_dir,
                worktree_git_dir_created_at_ns
            FROM branches
            WHERE repository_id = ?
            ORDER BY is_pinned DESC, position, id
        ))?(repository_id)
        .context("failed to list branches")?;

        rows.into_iter()
            .map(
                |(
                    id,
                    repository_id,
                    number,
                    name,
                    path,
                    base_ref,
                    position,
                    is_pinned,
                    archived_at,
                    provenance_columns,
                )| {
                    Ok(Branch {
                        id,
                        repository_id,
                        number,
                        name,
                        path,
                        base_ref,
                        position,
                        is_pinned,
                        archived_at,
                        worktree_provenance: worktree_provenance(provenance_columns)?,
                    })
                },
            )
            .collect()
    }
}

fn worktree_provenance(
    (git_dir, created_at_ns): (Option<PathBuf>, Option<i64>),
) -> Result<Option<WorktreeProvenance>> {
    match (git_dir, created_at_ns) {
        (None, None) => Ok(None),
        (Some(git_dir), Some(git_dir_created_at_ns)) => Ok(Some(WorktreeProvenance {
            git_dir,
            git_dir_created_at_ns,
        })),
        _ => bail!("branch worktree provenance is incomplete"),
    }
}

fn available_branch_name_and_path(
    connection: &sqlez::connection::Connection,
    repository_id: RepositoryId,
    repository_name: &str,
    requested_name: &str,
    paths: &VoidPaths,
) -> Result<(String, PathBuf)> {
    for suffix in 1_u64..=u64::MAX {
        let name = if suffix == 1 {
            requested_name.to_owned()
        } else {
            format!("{requested_name}-{suffix}")
        };
        let path = paths.branch_worktree(repository_name, &name)?;
        let exists =
            connection.select_row_bound::<(RepositoryId, String, PathBuf), bool>(sql!(
                SELECT EXISTS(
                    SELECT TRUE
                    FROM branches
                    WHERE (repository_id = ? AND name = ?) OR path = ?
                )
            ))?((repository_id, name.clone(), path.clone()))?
            .context("branch availability query returned no row")?;

        if !exists {
            return Ok((name, path));
        }
    }

    bail!("exhausted every branch name suffix")
}
