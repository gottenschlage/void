//! SQLite persistence for Void's workspace hierarchy.

mod branches;
mod repositories;
mod schema;
mod workspaces;

use std::path::{Path, PathBuf};

#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context as _, Result};
use sqlez::{
    bindable::{Bind, Column, StaticColumnCount},
    statement::Statement,
    thread_safe_connection::ThreadSafeConnection,
};

use crate::{git::WorktreeProvenance, paths::VoidPaths};

use self::schema::{CONNECTION_INITIALIZE_QUERY, DATABASE_INITIALIZE_QUERY, WorkspaceSchema};

macro_rules! database_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(i64);

        impl $name {
            pub const fn as_i64(self) -> i64 {
                self.0
            }

            #[cfg(test)]
            pub(crate) const fn from_i64(value: i64) -> Self {
                Self(value)
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
    /// Identity of the Git administration directory created or explicitly adopted by Void.
    pub worktree_provenance: Option<WorktreeProvenance>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewBranch {
    pub repository_id: RepositoryId,
    pub requested_name: String,
    pub base_ref: String,
    pub position: i64,
    pub is_pinned: bool,
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
}

#[cfg(test)]
mod tests;
