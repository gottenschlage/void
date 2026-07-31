use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use sqlez_macros::sql;

use crate::paths::validate_repository_name;

use super::{NewRepository, Repository, RepositoryId, WorkspaceDb, WorkspaceId};

impl WorkspaceDb {
    pub async fn add_repository(&self, repository: NewRepository) -> Result<RepositoryId> {
        validate_repository_name(&repository.name)?;
        if repository.position < 0 {
            bail!("repository position cannot be negative");
        }

        self.connection
            .write(move |connection| {
                connection
                    .select_row_bound::<(WorkspaceId, String, PathBuf, i64, bool), RepositoryId>(
                        sql!(
                            INSERT INTO repositories(
                                workspace_id,
                                name,
                                path,
                                position,
                                is_pinned
                            )
                            VALUES (?, ?, ?, ?, ?)
                            RETURNING id
                        ),
                    )?((
                    repository.workspace_id,
                    repository.name,
                    repository.path,
                    repository.position,
                    repository.is_pinned,
                ))?
                .context("repository insert did not return an id")
            })
            .await
            .context("failed to add repository")
    }

    /// Updates whether an active repository is pinned in workspace ordering.
    pub async fn set_repository_pinned(
        &self,
        repository_id: RepositoryId,
        is_pinned: bool,
    ) -> Result<()> {
        self.connection
            .write(move |connection| {
                connection.exec_bound::<(bool, RepositoryId)>(sql!(
                    UPDATE repositories
                    SET is_pinned = ?, updated_at = CURRENT_TIMESTAMP
                    WHERE id = ? AND archived_at IS NULL
                ))?((is_pinned, repository_id))
            })
            .await
            .context("failed to update repository pin")
    }

    /// Hides a repository from the active workspace without deleting its data.
    pub async fn archive_repository(&self, repository_id: RepositoryId) -> Result<()> {
        self.connection
            .write(move |connection| {
                connection.exec_bound::<RepositoryId>(sql!(
                    UPDATE repositories
                    SET archived_at = CURRENT_TIMESTAMP,
                        updated_at = CURRENT_TIMESTAMP,
                        is_pinned = FALSE
                    WHERE id = ? AND archived_at IS NULL
                ))?(repository_id)
            })
            .await
            .context("failed to archive repository")
    }

    /// Restores an archived repository to the active workspace.
    pub async fn unarchive_repository(&self, repository_id: RepositoryId) -> Result<()> {
        self.connection
            .write(move |connection| {
                connection.exec_bound::<RepositoryId>(sql!(
                    UPDATE repositories
                    SET archived_at = NULL, updated_at = CURRENT_TIMESTAMP
                    WHERE id = ? AND archived_at IS NOT NULL
                ))?(repository_id)
            })
            .await
            .context("failed to restore repository")
    }

    /// Persists the visible repository order within a workspace.
    pub async fn reorder_repositories(
        &self,
        workspace_id: WorkspaceId,
        repository_ids: Vec<RepositoryId>,
    ) -> Result<()> {
        self.connection
            .write(move |connection| {
                connection.with_savepoint("reorder_repositories", || {
                    for (position, repository_id) in repository_ids.into_iter().enumerate() {
                        connection.exec_bound::<(i64, RepositoryId, WorkspaceId)>(sql!(
                            UPDATE repositories
                            SET position = ?, updated_at = CURRENT_TIMESTAMP
                            WHERE id = ? AND workspace_id = ? AND archived_at IS NULL
                        ))?((
                            position as i64,
                            repository_id,
                            workspace_id,
                        ))?;
                    }
                    Ok(())
                })
            })
            .await
            .context("failed to reorder repositories")
    }

    pub fn repositories(&self, workspace_id: WorkspaceId) -> Result<Vec<Repository>> {
        let rows = self.connection.select_bound::<WorkspaceId, (
            RepositoryId,
            WorkspaceId,
            String,
            PathBuf,
            i64,
            bool,
            i64,
            Option<String>,
        )>(sql!(
            SELECT
                id,
                workspace_id,
                name,
                path,
                position,
                is_pinned,
                sequence,
                archived_at
            FROM repositories
            WHERE workspace_id = ?
            ORDER BY is_pinned DESC, position, id
        ))?(workspace_id)
        .context("failed to list repositories")?;

        Ok(rows
            .into_iter()
            .map(
                |(id, workspace_id, name, path, position, is_pinned, sequence, archived_at)| {
                    Repository {
                        id,
                        workspace_id,
                        name,
                        path,
                        position,
                        is_pinned,
                        sequence,
                        archived_at,
                    }
                },
            )
            .collect())
    }
}
