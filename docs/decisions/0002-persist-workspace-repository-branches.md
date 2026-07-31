# ADR 0002: Persist workspace repositories and managed branches

- **Status:** Accepted
- **Date:** 2026-07-30
- **Decision owners:** Void maintainers

## Context

Void's first product domain is one workspace containing local repositories. Each repository can run multiple coding agents concurrently, so every Void-managed branch must have a dedicated Git worktree. Persistence must preserve stable identity, display ordering, pinning, branch allocation, and archived history without treating SQLite as the authority for Git state.

Zed's desktop database persists workspace paths and presentation state, then discovers repositories, branches, and worktrees from Git. Zed distinguishes a repository's shared common directory from each linked worktree and supports attached and detached worktrees. Its hosted collaboration schema stores repository snapshots for synchronization, not as the local source of truth.

Void differs because it owns branch/worktree creation and needs stable integration numbers and archival records for agent work.

## Decision

Use SQLite through Zed's pinned `sqlez` implementation. Store the database at `<Void application data>/void.db`, where the application-data directory follows the host platform. Enable foreign keys, WAL mode, a 500 ms busy timeout, and normal synchronization, following Zed's database initialization.

Use three tables:

- `workspaces` stores workspace identity and timestamps.
- `repositories` belongs to a workspace and stores its local path, display position, pin state, branch-number sequence, archival time, and timestamps.
- `branches` belongs to a repository and represents one Void-managed Git branch plus its dedicated worktree path. It stores an immutable repository-scoped integration number, allocated Git name, base ref, display position, pin state, archival time, and timestamps.

The initial UI exposes one workspace, although the relational model does not require a destructive migration to support more later.

Repository and branch records are deleted transitively only when their owning workspace record is deleted. Filesystem and Git deletion are separate operations and must never be inferred from a database cascade.

## Branch and path allocation

A repository's `sequence` is incremented transactionally and becomes the new branch's immutable `number`. Numbers are never reused, including after archival.

A requested branch keeps its Git name when available. Slash characters are flattened only in the worktree directory:

```text
branch: feature/auth
path:   <Void application data>/worktrees/app/feature-auth
```

Names and paths remain reserved after archival. If either the requested Git name or flattened path was used, Void tries `-2`, `-3`, and so on. The chosen suffix applies to both the Git branch and worktree directory. Integration number and name suffix are independent.

Repository names are one path component and unique within a workspace, making the worktree root deterministic:

```text
<Void application data>/worktrees/<repository name>/<allocated branch name>
```

## Consequences

- Multiple agents can be mapped to distinct branch/worktree records without sharing a checkout.
- Pinning and ordering are persisted independently for repositories and branches.
- Archived names, paths, and integration numbers remain durable history.
- SQLite records allocation and intent; future Git integration must verify actual branch and worktree state before starting an agent.
- Branch reservation may precede Git creation. If Git creation fails, the reservation must be cleaned up without decrementing the repository sequence.
- Actual Git worktree creation, reconciliation, agent association, and safe cleanup are intentionally outside this persistence change.
- `sqlez`, `sqlez_macros`, and the async runtime patches required by the pinned Zed revision move together with that revision.

## References

Verified against Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/db/src/db.rs::AppDatabase` and database initialization pragmas
- `crates/workspace/src/persistence.rs::WorkspaceDb`
- `crates/project/src/git_store.rs::RepositorySnapshot`
- `crates/git/src/repository.rs::{Branch, Worktree, CreateWorktreeTarget}`
- `crates/recent_projects/src/recent_projects.rs::get_branch_for_worktree`
- `crates/collab/src/db/tables/project_repository.rs`

## Implementation note

Managed Git branches, dedicated worktrees, terminals, and safe deletion now exist, but no coding-agent runtime or agent-to-branch association has been implemented. References to agents above describe the motivating product model, not current executable behavior.

SQLite references:

- <https://www.sqlite.org/stricttables.html>
- <https://www.sqlite.org/foreignkeys.html>
- <https://www.sqlite.org/wal.html>
