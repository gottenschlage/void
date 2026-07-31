# ADR 0010: Validate and confirm managed-worktree deletion

- **Status:** Accepted
- **Date:** 2026-07-31
- **Decision owners:** Void maintainers

## Context

Void permanently deleted a branch by running `git worktree remove --force` and
`git branch -D` before deleting its SQLite row. That sequence could discard
modified or untracked files and unmerged commits after one generic prompt. A
stored path also did not prove that the directory was still the worktree Void
created.

Existing databases predate provenance metadata. A read-only inspection of the
maintainer's database found 15 such branch rows, so treating every old row as
newly verified would be unsafe while permanently refusing their deletion would
strand existing managed worktrees.

Zed separates worktree removal from branch deletion, passes force explicitly,
and retries with force only after a focused confirmation. Its managed-worktree
archive additionally constrains the path to its managed directory, verifies a
creation record against the Git administration directory, and releases project
entities before touching the filesystem.

## Considered options

1. Automatically trust every legacy branch row whose path resembles Void's
   current layout. This preserves behavior but does not establish provenance.
2. Refuse permanent deletion for every legacy row. This is safest but removes
   an existing operation for all current worktrees.
3. Use the exact typed branch-name confirmation as explicit one-time adoption,
   but only after all live Git and path checks pass. Record that identity before
   deletion and verify it again. This preserves compatibility without silently
   trusting legacy state.

## Decision

Void uses explicit one-time adoption for legacy rows.

Every permanent deletion follows this sequence:

1. Require the user to type the exact branch name.
2. Require the persisted branch to belong to the selected repository and its
   path to equal the deterministic `VoidPaths` location beneath the managed
   worktrees directory.
3. Release the root-owned terminal panel, context header, and live-diff
   registration. Await GPUI release of the terminal panel before running Git.
4. Canonicalize the repository and worktree, inspect `git worktree list
   --porcelain -z`, and require the exact registered path and
   `refs/heads/<branch>` identity.
5. Require the worktree and selected repository to resolve to the same Git
   common directory. Record the linked worktree's Git administration directory
   and its filesystem creation time. Existing provenance is immutable and must
   match; a legacy row records this value only after the exact confirmation,
   then repeats validation.
6. Run `git worktree remove` without force. Only Git's recognized
   modified-or-untracked refusal enables a separate **Force Delete** prompt;
   validation is repeated before the forced retry.
7. Run `git branch -d`. Only Git's recognized not-fully-merged refusal enables
   a separate **Force Delete** prompt before `git branch -D`.
8. Delete the SQLite row only after both Git steps succeed.

Provenance capture versus comparison, validated worktrees, and
worktree-removed branches are represented by `WorktreeProvenanceCheck`,
`ValidatedManagedWorktree`, and `RemovedManagedWorktree`. A `GitDeleteMode`
enum makes force explicit at call sites, and
`ManagedWorktreeError` exposes only the actionable dirty and unmerged variants
needed by the UI.

If the worktree was removed but a later step failed, a subsequent exact
confirmation may resume only when recorded provenance exists, the worktree path
and Git administration directory are absent, and the branch is not checked out
in another registered worktree. Errors state which destructive steps already
succeeded. Canceling either force prompt never performs that force operation.

## Consequences

- Clean, merged worktrees are deleted without force.
- Dirty/untracked files and unmerged commits require separate, specific force
  confirmations.
- A recreated or mismatched worktree is refused even when it occupies the old
  path.
- Legacy rows remain nullable in the append-only migration and are adopted only
  through the confirmed validation flow.
- Newly created worktrees record provenance before they are exposed to the
  workspace model.
- Git stderr classification remains best-effort because Git does not provide a
  machine-readable error kind for these refusals. Unrecognized or localized
  errors are surfaced and never enable force.
- Missing legacy worktrees cannot be adopted because there is no live identity
  to verify. They require manual reconciliation.

## References

Verified against local Zed commit
`5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`:

- `crates/git/src/repository.rs::{GitRepository::remove_worktree, GitRepository::delete_branch}`
- `crates/git_ui/src/worktree_picker.rs::{WorktreePickerDelegate::delete_worktree, force_delete_prompt_for_worktree_remove_error}`
- `crates/git_ui/src/branch_picker.rs::{BranchListDelegate::delete_at, force_delete_prompt_for_branch_delete_error}`
- `crates/agent_ui/src/thread_worktree_archive.rs::{build_root_plan, verify_created_by_zed, remove_root}`
- `crates/project/src/project.rs::{Project::wait_for_worktree_release, Project::remove_worktree}`
- `crates/gpui/src/app/context.rs::Context::observe_release`

Official Git documentation:

- <https://git-scm.com/docs/git-worktree>
- <https://git-scm.com/docs/git-branch>
- <https://git-scm.com/docs/git-rev-parse>
