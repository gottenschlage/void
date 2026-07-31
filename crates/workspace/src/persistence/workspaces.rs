use anyhow::{Context as _, Result};
use sqlez_macros::sql;

use super::{Workspace, WorkspaceDb, WorkspaceId};

impl WorkspaceDb {
    pub async fn create_workspace(&self, name: String) -> Result<WorkspaceId> {
        self.connection
            .write(move |connection| {
                connection.select_row_bound::<String, WorkspaceId>(
                    sql!(INSERT INTO workspaces(name) VALUES (?) RETURNING id),
                )?(name)?
                .context("workspace insert did not return an id")
            })
            .await
            .context("failed to create workspace")
    }

    pub fn first_workspace(&self) -> Result<Option<Workspace>> {
        let row = self.connection.select_row::<(WorkspaceId, String)>(sql!(
            SELECT id, name FROM workspaces ORDER BY id LIMIT 1
        ))?()
        .context("failed to load workspace")?;

        Ok(row.map(|(id, name)| Workspace { id, name }))
    }

    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        let rows = self.connection.select::<(WorkspaceId, String)>(sql!(
            SELECT id, name FROM workspaces ORDER BY id
        ))?()
        .context("failed to list workspaces")?;

        Ok(rows
            .into_iter()
            .map(|(id, name)| Workspace { id, name })
            .collect())
    }
}
