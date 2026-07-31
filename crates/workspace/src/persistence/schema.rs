use sqlez::domain::Domain;
use sqlez_macros::sql;

pub(super) const CONNECTION_INITIALIZE_QUERY: &str = sql!(
    PRAGMA foreign_keys = TRUE;
);

pub(super) const DATABASE_INITIALIZE_QUERY: &str = sql!(
    PRAGMA journal_mode = WAL;
    PRAGMA busy_timeout = 500;
    PRAGMA synchronous = NORMAL;
);

pub(super) struct WorkspaceSchema;

impl Domain for WorkspaceSchema {
    const NAME: &str = "WorkspaceDb";
    const MIGRATIONS: &[&str] = &[
        sql!(
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
        ),
        sql!(
            ALTER TABLE branches ADD COLUMN worktree_git_dir BLOB;
            ALTER TABLE branches ADD COLUMN worktree_git_dir_created_at_ns INTEGER;
        ),
    ];
}
