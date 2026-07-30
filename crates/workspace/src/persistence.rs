use std::path::{Path, PathBuf};

#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context as _, Result, bail};
use sqlez::{
    bindable::{Bind, Column, StaticColumnCount},
    domain::Domain,
    statement::Statement,
    thread_safe_connection::ThreadSafeConnection,
};
use sqlez_macros::sql;

use crate::paths::{VoidPaths, validate_repository_name};

const CONNECTION_INITIALIZE_QUERY: &str = sql!(
    PRAGMA foreign_keys = TRUE;
);

const DATABASE_INITIALIZE_QUERY: &str = sql!(
    PRAGMA journal_mode = WAL;
    PRAGMA busy_timeout = 500;
    PRAGMA synchronous = NORMAL;
);

macro_rules! database_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(i64);

        impl $name {
            pub const fn as_i64(self) -> i64 {
                self.0
            }
        }

        impl StaticColumnCount for $name {}

        impl Bind for $name {
            fn bind(&self, statement: &Statement, start_index: i32) -> Result<i32> {
                self.0.bind(statement, start_index)
            }
        }

        impl Column for $name {
            fn column(statement: &mut Statement, start_index: i32) -> Result<(Self, i32)> {
                i64::column(statement, start_index)
                    .map(|(value, next_index)| (Self(value), next_index))
                    .with_context(|| {
                        format!(
                            "failed to read {} at column {start_index}",
                            stringify!($name)
                        )
                    })
            }
        }
    };
}

database_id!(WorkspaceId);
database_id!(RepositoryId);
database_id!(BranchId);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Repository {
    pub id: RepositoryId,
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub path: PathBuf,
    pub position: i64,
    pub is_pinned: bool,
    pub sequence: i64,
    pub archived_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewRepository {
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub path: PathBuf,
    pub position: i64,
    pub is_pinned: bool,
}

/// A Void-managed Git branch and its dedicated worktree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Branch {
    pub id: BranchId,
    pub repository_id: RepositoryId,
    pub number: i64,
    pub name: String,
    pub path: PathBuf,
    pub base_ref: String,
    pub position: i64,
    pub is_pinned: bool,
    pub archived_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewBranch {
    pub repository_id: RepositoryId,
    pub requested_name: String,
    pub base_ref: String,
    pub position: i64,
    pub is_pinned: bool,
}

struct WorkspaceSchema;

impl Domain for WorkspaceSchema {
    const NAME: &str = "WorkspaceDb";
    const MIGRATIONS: &[&str] = &[sql!(
        CREATE TABLE workspaces (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL CHECK (length(trim(name)) > 0),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        ) STRICT;

        CREATE TABLE repositories (
            id INTEGER PRIMARY KEY,
            workspace_id INTEGER NOT NULL,
            name TEXT NOT NULL CHECK (length(trim(name)) > 0),
            path BLOB NOT NULL,
            position INTEGER NOT NULL CHECK (position >= 0),
            is_pinned INTEGER NOT NULL CHECK (is_pinned IN (0, 1)),
            sequence INTEGER NOT NULL DEFAULT(0) CHECK (sequence >= 0),
            archived_at TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (workspace_id) REFERENCES workspaces(id)
                ON DELETE CASCADE
                ON UPDATE CASCADE,
            UNIQUE (workspace_id, name),
            UNIQUE (workspace_id, path)
        ) STRICT;

        CREATE INDEX repositories_by_workspace
            ON repositories(workspace_id, is_pinned DESC, position, id);

        CREATE TABLE branches (
            id INTEGER PRIMARY KEY,
            repository_id INTEGER NOT NULL,
            number INTEGER NOT NULL CHECK (number > 0),
            name TEXT NOT NULL CHECK (length(trim(name)) > 0),
            path BLOB NOT NULL,
            base_ref TEXT NOT NULL CHECK (length(trim(base_ref)) > 0),
            position INTEGER NOT NULL CHECK (position >= 0),
            is_pinned INTEGER NOT NULL CHECK (is_pinned IN (0, 1)),
            archived_at TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (repository_id) REFERENCES repositories(id)
                ON DELETE CASCADE
                ON UPDATE CASCADE,
            UNIQUE (repository_id, number),
            UNIQUE (repository_id, name),
            UNIQUE (path)
        ) STRICT;

        CREATE INDEX branches_by_repository
            ON branches(repository_id, is_pinned DESC, position, id);
    )];
}

/// SQLite persistence for Void's workspace, repository, and branch hierarchy.
#[derive(Clone)]
pub struct WorkspaceDb {
    connection: ThreadSafeConnection,
}

impl gpui::Global for WorkspaceDb {}

impl WorkspaceDb {
    /// Opens `<Void application data>/void.db`.
    pub async fn open_default(paths: &VoidPaths) -> Result<Self> {
        Self::open(paths.database()).await
    }

    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create database directory {}", parent.display())
            })?;
        }

        let uri = path
            .to_str()
            .with_context(|| format!("database path is not valid UTF-8: {}", path.display()))?;
        let connection = Self::build(uri, true).await?;
        Ok(Self { connection })
    }

    async fn build(uri: &str, persistent: bool) -> Result<ThreadSafeConnection> {
        ThreadSafeConnection::builder::<WorkspaceSchema>(uri, persistent)
            .with_db_initialization_query(DATABASE_INITIALIZE_QUERY)
            .with_connection_initialize_query(CONNECTION_INITIALIZE_QUERY)
            .build()
            .await
            .with_context(|| format!("failed to open workspace database at {uri}"))
    }

    #[cfg(test)]
    async fn open_test() -> Result<Self> {
        static NEXT_DATABASE_ID: AtomicU64 = AtomicU64::new(0);

        let id = NEXT_DATABASE_ID.fetch_add(1, Ordering::Relaxed);
        let uri = format!("void-workspace-test-{}-{id}", std::process::id());
        let connection = Self::build(&uri, false).await?;
        Ok(Self { connection })
    }

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
                    })
                })
            })
            .await
            .context("failed to reserve branch")
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
                archived_at
            FROM branches
            WHERE repository_id = ?
            ORDER BY is_pinned DESC, position, id
        ))?(repository_id)
        .context("failed to list branches")?;

        Ok(rows
            .into_iter()
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
                )| Branch {
                    id,
                    repository_id,
                    number,
                    name,
                    path,
                    base_ref,
                    position,
                    is_pinned,
                    archived_at,
                },
            )
            .collect())
    }
}

fn available_branch_name_and_path(
    connection: &sqlez::connection::Connection,
    repository_id: RepositoryId,
    repository_name: &str,
    requested_name: &str,
    paths: &VoidPaths,
) -> Result<(String, PathBuf)> {
    for suffix in 1_u64.. {
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

    unreachable!("the branch suffix counter is unbounded")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths() -> VoidPaths {
        VoidPaths::from_data_dir("/data/Void")
    }

    #[test]
    fn persists_the_approved_hierarchy_and_sort_order() {
        pollster::block_on(async {
            let db = WorkspaceDb::open_test().await?;
            let workspace_id = db.create_workspace("Void".into()).await?;
            assert_eq!(
                db.first_workspace()?,
                Some(Workspace {
                    id: workspace_id,
                    name: "Void".into(),
                })
            );
            let repository_id = db
                .add_repository(NewRepository {
                    workspace_id,
                    name: "app".into(),
                    path: "/code/app".into(),
                    position: 4,
                    is_pinned: true,
                })
                .await?;
            let branch = db
                .reserve_branch(
                    NewBranch {
                        repository_id,
                        requested_name: "feature/auth".into(),
                        base_ref: "refs/heads/main".into(),
                        position: 2,
                        is_pinned: true,
                    },
                    &test_paths(),
                )
                .await?;

            assert_eq!(branch.number, 1);
            assert_eq!(branch.name, "feature/auth");
            assert_eq!(
                branch.path,
                Path::new("/data/Void/worktrees/app/feature-auth")
            );
            assert_eq!(db.repositories(workspace_id)?[0].sequence, 1);
            assert_eq!(db.branches(repository_id)?, vec![branch]);
            Ok::<_, anyhow::Error>(())
        })
        .unwrap();
    }

    #[test]
    fn archived_branch_names_are_never_reused() {
        pollster::block_on(async {
            let db = WorkspaceDb::open_test().await?;
            let workspace_id = db.create_workspace("Void".into()).await?;
            let repository_id = db
                .add_repository(NewRepository {
                    workspace_id,
                    name: "app".into(),
                    path: "/code/app".into(),
                    position: 0,
                    is_pinned: false,
                })
                .await?;
            let first = db
                .reserve_branch(
                    NewBranch {
                        repository_id,
                        requested_name: "fix-auth".into(),
                        base_ref: "refs/heads/main".into(),
                        position: 0,
                        is_pinned: false,
                    },
                    &test_paths(),
                )
                .await?;
            db.archive_branch(first.id).await?;
            let second = db
                .reserve_branch(
                    NewBranch {
                        repository_id,
                        requested_name: "fix-auth".into(),
                        base_ref: "refs/heads/main".into(),
                        position: 1,
                        is_pinned: false,
                    },
                    &test_paths(),
                )
                .await?;

            assert_eq!(first.number, 1);
            assert_eq!(second.number, 2);
            assert_eq!(second.name, "fix-auth-2");
            assert_eq!(
                second.path,
                Path::new("/data/Void/worktrees/app/fix-auth-2")
            );
            Ok::<_, anyhow::Error>(())
        })
        .unwrap();
    }

    #[test]
    fn repository_menu_state_is_persisted_without_deleting_the_repository() {
        pollster::block_on(async {
            let db = WorkspaceDb::open_test().await?;
            let workspace_id = db.create_workspace("Void".into()).await?;
            let repository_id = db
                .add_repository(NewRepository {
                    workspace_id,
                    name: "app".into(),
                    path: "/code/app".into(),
                    position: 0,
                    is_pinned: false,
                })
                .await?;

            db.set_repository_pinned(repository_id, true).await?;
            assert!(db.repositories(workspace_id)?[0].is_pinned);

            db.archive_repository(repository_id).await?;
            let repository = &db.repositories(workspace_id)?[0];
            assert!(!repository.is_pinned);
            assert!(repository.archived_at.is_some());

            db.unarchive_repository(repository_id).await?;
            assert!(db.repositories(workspace_id)?[0].archived_at.is_none());
            Ok::<_, anyhow::Error>(())
        })
        .unwrap();
    }

    #[test]
    fn persists_repository_and_branch_drag_order() {
        pollster::block_on(async {
            let db = WorkspaceDb::open_test().await?;
            let workspace_id = db.create_workspace("Void".into()).await?;
            let first_repository = db
                .add_repository(NewRepository {
                    workspace_id,
                    name: "first".into(),
                    path: "/code/first".into(),
                    position: 0,
                    is_pinned: false,
                })
                .await?;
            let second_repository = db
                .add_repository(NewRepository {
                    workspace_id,
                    name: "second".into(),
                    path: "/code/second".into(),
                    position: 1,
                    is_pinned: false,
                })
                .await?;
            db.reorder_repositories(workspace_id, vec![second_repository, first_repository])
                .await?;
            assert_eq!(db.repositories(workspace_id)?[0].name, "second");

            let first_branch = db
                .reserve_branch(
                    NewBranch {
                        repository_id: first_repository,
                        requested_name: "first-branch".into(),
                        base_ref: "main".into(),
                        position: 0,
                        is_pinned: false,
                    },
                    &test_paths(),
                )
                .await?;
            let second_branch = db
                .reserve_branch(
                    NewBranch {
                        repository_id: first_repository,
                        requested_name: "second-branch".into(),
                        base_ref: "main".into(),
                        position: 1,
                        is_pinned: false,
                    },
                    &test_paths(),
                )
                .await?;
            db.reorder_branches(first_repository, vec![second_branch.id, first_branch.id])
                .await?;
            assert_eq!(db.branches(first_repository)?[0].name, "second-branch");
            Ok::<_, anyhow::Error>(())
        })
        .unwrap();
    }

    #[test]
    fn flattened_path_collisions_use_the_same_suffix_rule() {
        pollster::block_on(async {
            let db = WorkspaceDb::open_test().await?;
            let workspace_id = db.create_workspace("Void".into()).await?;
            let repository_id = db
                .add_repository(NewRepository {
                    workspace_id,
                    name: "app".into(),
                    path: "/code/app".into(),
                    position: 0,
                    is_pinned: false,
                })
                .await?;

            db.reserve_branch(
                NewBranch {
                    repository_id,
                    requested_name: "feature/auth".into(),
                    base_ref: "refs/heads/main".into(),
                    position: 0,
                    is_pinned: false,
                },
                &test_paths(),
            )
            .await?;
            let collision = db
                .reserve_branch(
                    NewBranch {
                        repository_id,
                        requested_name: "feature-auth".into(),
                        base_ref: "refs/heads/main".into(),
                        position: 1,
                        is_pinned: false,
                    },
                    &test_paths(),
                )
                .await?;

            assert_eq!(collision.name, "feature-auth-2");
            assert_eq!(
                collision.path,
                Path::new("/data/Void/worktrees/app/feature-auth-2")
            );
            Ok::<_, anyhow::Error>(())
        })
        .unwrap();
    }
}
