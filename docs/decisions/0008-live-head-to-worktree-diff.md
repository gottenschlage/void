# ADR 0008: Live HEAD-to-worktree diff header

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Void maintainers

## Context

Every active Void branch has a dedicated managed worktree. The sidebar and
active branch header need a live summary of its uncommitted line changes. Void
does not yet own Zed's editor-oriented
`fs`/`worktree`/`project` stack.

Zed's Git panel computes per-file statistics with
`git diff --numstat --no-renames HEAD`, refreshes them after coalesced worktree
events, and notifies observing GPUI entities only when repository state
changes.

## Decision

Use the same `HEAD`-to-worktree diff meaning as Zed's Git panel. Counts include
staged and unstaged tracked changes. Commits move `HEAD` and therefore reset
the count. Untracked files are excluded until staged, and binary files do not
contribute line totals.

Keep Git parsing and execution in the `workspace` crate without GPUI
dependencies. The Void application owns one private live-diff entity for every
repository with an active branch. That entity:

- starts an independent initial refresh for every registered branch;
- runs cancellable asynchronous Git commands without blocking GPUI's
  foreground executor;
- uses one watcher for shared Git metadata and all registered worktrees;
- uses the same `notify` version as the pinned Zed revision;
- coalesces filesystem events and permits at most one running refresh plus one
  pending follow-up per branch;
- retries watcher setup and runtime failures with bounded exponential backoff;
  and
- owns its watcher and tasks so archiving or deleting the final active branch
  cancels the repository lifecycle.

The header shows `#<number> <base-ref>/<branch-name>`. The base ref is
informational and does not affect the diff calculation. Loading, clean, and
initial-error states show no counter and add no new controls. A later refresh
error preserves the last successful counter.

Each sidebar branch row shows the same counter by default. Hovering the row
hides the counter and reveals the existing archive and delete actions in its
place.

## Consequences

- Multiple active branches refresh independently without blocking GPUI's
  foreground executor or installing duplicate shared-metadata watchers.
- Void gains the requested behavior without importing Zed's broader project
  subsystem or creating a speculative general Git store.
- Filesystem watcher failures do not block the initial count, preserve the last
  successful count, are logged, and retry automatically.

## References

Verified against Zed commit `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/git/src/repository.rs::GitRepository::diff_stat`
- `crates/git/src/status.rs::{DiffStat, GitDiffStat, parse_numstat}`
- `crates/project/src/git_store.rs::{Repository::paths_changed, compute_snapshot}`
- `crates/ui/src/components/diff_stat.rs::DiffStat`

Current documentation:

- <https://gpui.rs/>
- <https://docs.rs/gpui/latest/gpui/struct.Task.html>
- <https://docs.rs/notify/9.0.0-rc.4/notify/>
- <https://git-scm.com/docs/git-diff>
